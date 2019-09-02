extern crate slog;
extern crate structopt;

#[macro_use]
extern crate op_log;

use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, Utc};
use futures::Future;
use structopt::StructOpt;
use url::Url;

use crate::slog::Drain;
use display::DisplayFormat;
use futures::stream::Stream;
use kg_diag::BasicDiag;
use op_exec::OutcomeFuture;
use op_exec::{ConfigRef, Context as ExecContext, EngineRef, ModelPath};
use op_exec::{SshAuth, SshDest};
use op_log::{build_cli_drain, build_file_drain};
use options::*;
use slog::Duplicate;
use slog::FnValue;
use std::sync::atomic::*;
use std::sync::Arc;
use tokio::runtime;
mod display;
mod options;

pub static SHORT_VERSION: &str = env!("OP_SHORT_VERSION");
pub static LONG_VERSION: &str = env!("OP_LONG_VERSION");
pub static TIMESTAMP: &str = env!("OP_TIMESTAMP");

fn make_model_path_absolute(path: &mut ModelPath) {
    if let ModelPath::Path(ref mut path) = path {
        *path = path.canonicalize().unwrap();
    }
}

fn make_path_absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap()
}

fn init_logger(config: &ConfigRef, verbosity: u8) -> slog::Logger {
    let file_drain = build_file_drain(
        config.log().log_path().to_path_buf(),
        (*config.log().level()).into(),
    );
    let cli_drain = build_cli_drain(verbosity);

    let drain = Duplicate::new(file_drain, cli_drain);

    let logger = slog::Logger::root(
        drain.fuse(),
        o!("module" =>
         FnValue(move |info| {
              info.module()
         }),
         "thread" =>
         FnValue(move |_info| {
              std::thread::current().name().unwrap().to_string()
         })
        ),
    );
    logger
}
/// start engine and execute provided operation. Returns exit code
fn local_run(
    current_dir: PathBuf,
    config: ConfigRef,
    operation: ExecContext,
    disp_format: DisplayFormat,
    verbose: u8,
) -> Result<u32, BasicDiag> {
    let logger = init_logger(&config, verbose);

    let engine = EngineRef::start(current_dir, config, logger.clone())?;
    let outcome_fut: OutcomeFuture = engine.enqueue_operation(operation.into(), false)?;

    let progress_fut = outcome_fut.progress().for_each(|p| {
        println!("=========================================");
        eprintln!("Total: {}/{} {:?}", p.value(), p.max(), p.unit());
        for p in p.steps() {
            if let Some(ref file_name) = p.file_name() {
                eprintln!("{}/{} {:?}: {}", p.value(), p.max(), p.unit(), file_name);
            } else {
                eprintln!("Step value: {}/{} {:?}", p.value(), p.max(), p.unit());
            }
        }
        //            eprintln!("p = {:#?}", p);
        Ok(())
    });

    let progress_fut = progress_fut.map_err(|err| {
        eprintln!("progress error \n{}", err);
    });

    let engine_fut = engine.clone().then(|_| {
        // Nothing to do when engine future complete
        futures::future::ok(())
    });

    let exit_code = Arc::new(AtomicU32::new(0));
    let code = exit_code.clone();
    let outcome_fut = outcome_fut
        .and_then(move |outcome| {
            display::display_outcome(&outcome, disp_format);
            futures::future::ok(())
        })
        .map_err(move |err| {
            use kg_diag::Diag;
            code.store(err.detail().code(), Ordering::Relaxed);
            error!(logger, "Operation execution error = {}", err; "verbosity" => 0);
        })
        .then(move |_| {
            engine.stop();
            futures::future::ok::<(), ()>(())
        });

    let mut rt = runtime::Builder::new().build().unwrap();
    rt.block_on(
        outcome_fut
            .join3(engine_fut, progress_fut)
            .then(|_| futures::future::ok::<(), ()>(())),
    )
    .unwrap();
    Ok(exit_code.load(Ordering::Relaxed))
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

    let model_dir_path = PathBuf::from(model_dir_path)
        .canonicalize()
        .expect("Cannot find model directory");

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
        Command::Exec { path } => {
            make_path_absolute(&path);

            ExecContext::ProcExec { exec_path: path }
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
        Command::Init { path } => ExecContext::ModelInit {
            path: path.canonicalize().expect("Error resolving path"),
        },
        Command::Remote {
            expr,
            command,
            model,
        } => {
            let command = command.join(" ");
            ExecContext::RemoteExec {
                expr,
                command,
                model_path: model,
            }
        }
    };

    let res = local_run(model_dir_path, config, cmd, disp_format, verbose);

    let exit_code = match res {
        Ok(code) => code as i32,
        Err(err) => {
            eprintln!("{}", err);
            -1
        }
    };
    std::process::exit(exit_code)
}
