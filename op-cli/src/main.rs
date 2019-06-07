#[macro_use]
extern crate structopt;

#[macro_use]
extern crate slog;

use slog::Drain;
use slog::FnValue;

use structopt::StructOpt;
use url::Url;

use op_exec::OutcomeFuture;
use op_exec::{ConfigRef, Context as ExecContext, EngineRef, ModelPath};
use op_exec::{SshDest, SshAuth};

use std::fs;
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

use actix::prelude::*;
use uuid::Uuid;

use futures::Future;

mod display;
mod options;

use display::DisplayFormat;
use options::*;

use chrono::{Utc, DateTime, FixedOffset};
use chrono::offset::TimeZone;

pub static SHORT_VERSION: &'static str = env!("OP_SHORT_VERSION");
pub static LONG_VERSION: &'static str = env!("OP_LONG_VERSION");
pub static TIMESTAMP: &'static str = env!("OP_TIMESTAMP");

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

/// start engine and execute provided operation
fn local_run(model_dir: PathBuf, config: ConfigRef, operation: ExecContext, disp_format: DisplayFormat) {
    let logger = init_file_logger(&config);

    let engine = check(EngineRef::start(model_dir, config, logger.clone()));
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
            display::display_outcome(&outcome, disp_format);
            futures::future::ok(())
        })
        .map_err(move |err| {
            error!(logger, "Operation execution error = {:?}", err);
        })
        .then(move |_| {
            engine.stop();
            futures::future::ok(())
        });
    Arbiter::spawn(outcome_fut);
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
    let ts_local: DateTime<FixedOffset> = DateTime::parse_from_rfc3339(TIMESTAMP).unwrap();
    let ts_utc = ts_local.with_timezone(&Utc);
    let matches = Opts::clap()
        .version(SHORT_VERSION)
        .long_version(format!("{} ({})", LONG_VERSION, ts_utc.format("%F %T")).as_str())
        .get_matches();

    let Opts {
        config_file_path,
        model_dir_path,
        command,
        verbose,
    } = Opts::from_clap(&matches);

    let model_dir_path = PathBuf::from(model_dir_path).canonicalize().expect("Cannot find model directory");

    let config = match ConfigRef::read(&config_file_path) {
        Err(err) => {
            println!("Cannot read config file {} : {:?}", config_file_path, err);
            return;
        }
        Ok(c) => c,
    };

    let mut disp_format = DisplayFormat::Json;

    let cmd: ExecContext = match command {
        //////////////////////////////// CLI client options ////////////////////////////////
        Command::Config { format } => {
            disp_format = format;

            ExecContext::ConfigGet
        }
        Command::List { format } => {
            disp_format = format;

            ExecContext::ModelList
        }
        Command::Commit { message } => {
            disp_format = DisplayFormat::Text;
            ExecContext::ModelCommit(message)
        }
        Command::Query {
            expr,
            mut model,
            format,
        } => {
            disp_format = format;

            make_model_path_absolute(&mut model);

            ExecContext::ModelQuery { model, expr }
        }
        Command::Test { format, mut model } => {
            disp_format = format;

            make_model_path_absolute(&mut model);

            ExecContext::ModelTest { model }
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
            }
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
            }
        }
        Command::Exec { mut path } => {
            make_path_absolute(&mut path);

            ExecContext::ProcExec {
                bin_id: Uuid::nil(),
                exec_path: path,
            }
        }
        Command::Check {
            mut model,
            filter,
            dry_run,
        } => {
            make_model_path_absolute(&mut model);

            ExecContext::ModelCheck {
                model,
                filter,
                dry_run,
            }
        }
        Command::Probe {
            mut model,
            url,
            password,
            identity_file,
            filter,
            args,
        } => {
            make_model_path_absolute(&mut model);

            let ssh_auth = if let Some(password) = password {
                SshAuth::Password { password }
            } else if let Some(identity_file) = identity_file {
                SshAuth::PublicKey { identity_file }
            } else {
                SshAuth::Default
            };

            let ssh_dest = SshDest::from_url(&url, ssh_auth);

            ExecContext::ModelProbe {
                ssh_dest,
                model,
                filter,
                args,
            }
        }
        Command::Init => {
            ExecContext::ModelInit
        }
    };

    actix::System::run(move || {
        local_run(model_dir_path, config, cmd, disp_format);
    });
}
