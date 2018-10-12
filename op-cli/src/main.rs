#[macro_use]
extern crate structopt;
extern crate daemonize;

extern crate tokio;
extern crate tokio_codec;
extern crate tokio_signal;
extern crate tokio_uds;

extern crate bytes;
extern crate chrono;
extern crate futures;
extern crate uuid;

extern crate rmp_serde as rmps;
#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate toml;

extern crate actix;

extern crate kg_tree;
extern crate kg_diag;
extern crate op_exec;
extern crate op_net;

extern crate env_logger;

#[macro_use]
extern crate slog;
extern crate linked_hash_map;
extern crate slog_async;
extern crate slog_term;
extern crate core;

use slog::Drain;
use slog::FnValue;

use structopt::StructOpt;

use op_exec::OutcomeFuture;
use op_exec::{ConfigRef, Context as ExecContext, EngineRef, ModelPath};

use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use actix::prelude::*;
use daemonize::Daemonize;
use uuid::Uuid;

use futures::Future;

mod cli_client;
mod cli_server;
mod commons;
mod daemon;
mod options;

use options::*;

use cli_client::client::Client;
use cli_client::display::DisplayFormat;

use commons::CliMessage;
use daemon::Daemon;

fn check<T, E: std::fmt::Display>(result: Result<T, E>) -> T {
    match result {
        Ok(t) => t,
        Err(err) => {
            eprintln!("Error starting operon: {}", err);
            std::process::exit(-1);
        }
    }
}

fn make_model_path_absolute(path: &mut ModelPath) {
    match path {
        ModelPath::Path(ref mut path) => {
            *path = path.canonicalize().unwrap();
        }
        _ => {}
    }
}

fn make_path_absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap()
}

/// # Returns
/// true if pid file already exists
fn check_or_create(path: &Path) -> bool {
    let exists = path.exists();
    if !exists {
        // create dir if not exists
        fs::create_dir_all(path.parent().unwrap()).expect("Cannot create pid dir")
    }
    exists
}

fn kill(path: &Path) {
    use std::io::Read;
    let mut pid = String::new();

    let mut pid_file = File::open(path).expect("Cannot open pid file");
    pid_file
        .read_to_string(&mut pid)
        .expect("Cannot read pid file");

    // replace with libc::kill?
    use std::process::Command;
    let out = Command::new("kill")
        .args(&["-s", "15", pid.as_str()]) // SIGTERM
        .output()
        .expect("Cannot spawn kill command");

    if !out.status.success() {
        eprintln!("Error : {:?}", out);
    }
}

/// start engine and execute provided operation
fn local_run(config: ConfigRef, operation: ExecContext, disp_format: DisplayFormat) {
    let logger = init_file_logger(&config);

    let engine = check(EngineRef::start(config, logger.clone()));
    let outcome_fut: OutcomeFuture = engine
        .enqueue_operation(operation.into(), false)
        .expect("Cannot enqueue operation");

    Arbiter::spawn(engine.clone().then(|_| {
        // Nothing to do when engine future complete
        System::current().stop();
        futures::future::ok(())
    }));

    let outcome_fut = outcome_fut
        .and_then(move |outcome| {
            cli_client::display::display_outcome(&outcome, disp_format);
            futures::future::ok(())
        }).map_err(move |err| {
            error!(logger, "Operation execution error = {:?}", err);
        }).then(move |_| {
            engine.stop();
            futures::future::ok(())
        });
    Arbiter::spawn(outcome_fut);
}

fn run_as_daemon(pid_path: &Path, config: &ConfigRef) {
    let mut open_opts = OpenOptions::new();

    open_opts.create(true).append(true);

    // pipe stdio to log file, just in case
    let log_file = open_opts
        .open(config.log().log_path())
        .expect("Cannot open log file");

    let err_file: daemonize::Stdio = log_file.try_clone().unwrap().into();

    let log_file: daemonize::Stdio = log_file.into();

    // Enable backtraces
    std::env::set_var("RUST_BACKTRACE", "1");

    if let Err(err) = Daemonize::new()
        .pid_file(pid_path)
        .chown_pid_file(true)
        .working_directory("./")
        .stdout(log_file)
        .stderr(err_file)
        .start()
    {
        eprintln!("Cannot start Operon service: {}", err);
        std::process::exit(-1);
    }
}

fn init_term_logger() -> slog::Logger {
    let decorator = slog_term::TermDecorator::new().build();
    let drain = slog_term::FullFormat::new(decorator).build().fuse();
    let drain = slog_async::Async::new(drain).build().fuse();

    let logger = slog::Logger::root(
        drain,
        o!("module" =>
           FnValue(move |info| {
                info.module()
           })
          ),
    );
    logger
}

fn init_file_logger(config: &ConfigRef) -> slog::Logger {
    let log_path = config.log().log_path();
    if let Some(log_dir) = log_path.parent() {
        fs::create_dir_all(log_dir).expect("Cannot create log dir");
    }

    let mut open_opts = OpenOptions::new();

    open_opts.create(true).append(true);

    let log_file = open_opts.open(log_path).expect("Cannot open log file");

    let decorator = slog_term::PlainSyncDecorator::new(log_file.try_clone().unwrap());
    let drain = slog_term::FullFormat::new(decorator).build();
    let drain = slog::LevelFilter::new(drain, (*config.log().level()).into());

    let logger = slog::Logger::root(
        drain.fuse(),
        o!("module" =>
           FnValue(move |info| {
                info.module()
           })
          ),
    );
    logger
}

fn main() {
    let Opts {
        config_file_path,
        local,
        remote,
        command,
        verbose,
    } = Opts::from_args();

    let config = match ConfigRef::read(&config_file_path) {
        Err(err) => {
            println!("Cannot read config file {} : {:?}", config_file_path, err);
            return;
        }
        Ok(c) => c,
    };

    env_logger::init();

    let mut disp_format = DisplayFormat::Json;

    let cmd: CliMessage = match command {
        //////////////////////////////// Daemon options ////////////////////////////////
        Command::Start { foreground, } => {
            if check_or_create(config.daemon().pid_file_path()) {
                println!("Operon already running!");
                return;
            }
            println!("Starting operon service");

            let logger;

            if foreground {
                // running in foreground, just create pid file
                let mut pid_file = File::create(config.daemon().pid_file_path()).expect("Cannot create pid file");
                use std::io::Write;
                pid_file
                    .write(std::process::id().to_string().as_bytes())
                    .expect("Cannot write pid file");

                logger = init_term_logger();
            } else {
                logger = init_file_logger(&config);
                run_as_daemon(config.daemon().pid_file_path(), &config);
            }

            info!(logger, "Operon started");
            let log = logger.clone();

            // Catch panic and remove pid file
            let result = {
                let config = config.clone();
                std::panic::catch_unwind(|| {
                    actix::System::run(move || {
                        Daemon::run(config, logger);
                    });
                })
            };

            // System stopped, remove pid file
            if let Err(err) = std::fs::remove_file(config.daemon().pid_file_path()) {
                error!(log, "Cannot remove pid file {:?}", err)
            };

            if result.is_ok() {
                info!(log, "Operon system stopped");
            } else {
                error!(log, "Operon closed abnormally");
            }

            return;
        }
        Command::Stop => {
            if config.daemon().pid_file_path().exists() {
                kill(config.daemon().pid_file_path());
                println!("Stopping operon service");
            } else {
                println!("There is no running Operon service!");
            }
            return;
        }
        //////////////////////////////// Multi instance options ////////////////////////////////
        Command::Nodes => {
            CliMessage::GetReachableInstances
        }
        //////////////////////////////// CLI client options ////////////////////////////////
        Command::Config { format } => {
            disp_format = format;

            ExecContext::ConfigGet.into()
        }
        Command::List { format } => {
            disp_format = format;

            ExecContext::ModelList.into()
        }
        Command::Store { path } => {
            disp_format = DisplayFormat::Text;
            let mut model_path = PathBuf::from(".");

            if let Some(path) = path {
                model_path = make_path_absolute(&path);
            } else {
                model_path = model_path
                    .canonicalize()
                    .expect("Cannot canonicalize path.")
            }

            ExecContext::ModelStore(model_path).into()
        }
        Command::Query {
            expr,
            mut model,
            format,
        } => {
            disp_format = format;

            make_model_path_absolute(&mut model);

            ExecContext::ModelQuery { model, expr }.into()
        }
        Command::Test {
            format,
            mut model,
        } => {
            disp_format = format;

            make_model_path_absolute(&mut model);

            ExecContext::ModelTest { model }.into()
        }
        Command::Diff {
            format,
            mut source,
            mut target,
            method,
        } => {
            // FIXME fails when id provided instead of path (because of canonicalize)
            disp_format = format;

            make_model_path_absolute(&mut source);
            make_model_path_absolute(&mut target);

            ExecContext::ModelDiff {
                prev_model: source,
                next_model: target,
                method,
            }.into()
        }
        Command::Update {
            format,
            mut source,
            mut target,
            dry_run,
        } => {
            disp_format = format;

            make_model_path_absolute(&mut source);
            make_model_path_absolute(&mut target);

            ExecContext::ModelUpdate {
                prev_model: source,
                next_model: target,
                dry_run,
            }.into()
        }
        Command::Exec { mut path } => {
            make_path_absolute(&mut path);

            ExecContext::ExecWork { bin_id: Uuid::nil(), work_path: path }.into()
        }
        Command::Check { mut model, filter, dry_run } => {
            make_model_path_absolute(&mut model);

            ExecContext::ModelCheck { model, filter, dry_run }.into()
        }
    };

    actix::System::run(move || match cmd {
        CliMessage::GetReachableInstances => {
            Client::connect_and_send(config.daemon().socket_path(), CliMessage::GetReachableInstances, disp_format);
        }
        CliMessage::Execute(operation) => match (local, remote) {
            (true, false) => {
                local_run(config, operation, disp_format);
            }
            (false, true) => {
                Client::connect_and_send(config.daemon().socket_path(), operation.into(), disp_format);
            }
            (false, false) => {
                if config.daemon().socket_path().exists() {
                    Client::connect_and_send(config.daemon().socket_path(), operation.into(), disp_format);
                } else {
                    local_run(config, operation, disp_format);
                }
            }
            (true, true) => {
                println!("Cannot use '--remote' and '--local' flags at the same time.");
                System::current().stop()
            }
        },
        CliMessage::Cancel => unimplemented!(),
    });
}
