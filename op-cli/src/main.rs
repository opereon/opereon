extern crate slog;
extern crate structopt;

#[macro_use]
extern crate op_log;

use op_core::*;
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, Utc};
use futures::Future;
use structopt::StructOpt;
use url::Url;

use crate::slog::Drain;
use display::DisplayFormat;
use futures::stream::Stream;
use kg_diag::{BasicDiag, Diag};
use op_rev::{RevPath};
use op_log::{build_file_drain, CliLogger};
use options::*;
use slog::{FnValue, o, error};
use std::sync::atomic::*;
use std::sync::Arc;
use tokio::runtime;
use op_core::config::ConfigRef;
use op_core::context::Context as ExecContext;
use op_engine::EngineRef;
use op_core::outcome::Outcome;

mod display;
mod options;

pub static SHORT_VERSION: &str = env!("OP_SHORT_VERSION");
pub static LONG_VERSION: &str = env!("OP_LONG_VERSION");
pub static TIMESTAMP: &str = env!("OP_TIMESTAMP");


fn make_path_absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap()
}

fn init_logger(config: &ConfigRef, verbosity: u8) -> slog::Logger {
    let file_drain = build_file_drain(
        config.log().log_path().to_path_buf(),
        (*config.log().level()).into(),
    );

    let logger = slog::Logger::root(
        file_drain.fuse(),
        o!("module" =>
         FnValue(move |info| {
              info.module()
         })
        ),
    );

    let cli_logger = CliLogger::new(verbosity as usize, logger.new(o!()));
    op_log::set_logger(cli_logger);
    logger
}

/// start engine and execute provided operation. Returns exit code
fn local_run(
    current_dir: PathBuf,
    config: ConfigRef,
    ctx: ExecContext,
    disp_format: DisplayFormat,
    verbose: u8,
) -> Result<u32, BasicDiag> {
    let logger = init_logger(&config, verbose);

    let mut rt = EngineRef::<()>::build_runtime();

    let out_res = rt.block_on(async {
        let services = init_services(current_dir, config, logger).await;

        let engine = EngineRef::with_services(services);

        let e = engine.clone();
        let res = tokio::spawn(async move {
            let res = e.enqueue_with_res(ctx.into()).await;
            e.stop();
            res
        });
        engine.register_progress_cb(|_e, o| {
            if !o.read().progress().is_done() {
                println!("{}", o.read().progress())
            }
        });
        let (_engine_result, res) = futures::future::join(engine.start(), res).await;
        let op_res = res.unwrap();
        op_res
    });

    let outcome = out_res?;

    display::display_outcome(&outcome, disp_format);
    Ok(0)
/*
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
            error!(logger, "Operation execution error"; "err" => err.to_string());
            op_error!(0, "Operation execution error:\n{}", err);
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
    */
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
        // Command::Config { format } => {
        //     disp_format = format;
        //
        //     ExecContext::ConfigGet
        // }
        // Command::Commit { message } => {
        //     disp_format = DisplayFormat::Text;
        //     ExecContext::ModelCommit(message)
        // }
        Command::Query {
            expr,
            model,
            format,
        } => {
            disp_format = format;
            ExecContext::ModelQuery { model, expr }
        }
        // Command::Test { format, model } => {
        //     disp_format = format;
        //     ExecContext::ModelTest { model }
        // }
        // Command::Diff {
        //     format,
        //     source,
        //     target,
        // } => {
        //     // FIXME fails when id provided instead of path (because of canonicalize)
        //     disp_format = format;
        //
        //     ExecContext::ModelDiff {
        //         prev_model: source,
        //         next_model: target,
        //     }
        // }
        // Command::Update {
        //     format,
        //     source,
        //     target,
        //     dry_run,
        // } => {
        //     disp_format = format;
        //     ExecContext::ModelUpdate {
        //         prev_model: source,
        //         next_model: target,
        //         dry_run,
        //     }
        // }
        // Command::Exec { path } => {
        //     make_path_absolute(&path);
        //     ExecContext::ProcExec { exec_path: path }
        // }
        // Command::Check {
        //     model,
        //     filter,
        //     dry_run,
        // } => {
        //     ExecContext::ModelCheck {
        //         model,
        //         filter,
        //         dry_run,
        //     }
        // }
        // Command::Probe {
        //     model,
        //     url,
        //     password,
        //     identity_file,
        //     filter,
        //     args,
        // } => {
        //     let ssh_auth = if let Some(password) = password {
        //         SshAuth::Password { password }
        //     } else if let Some(identity_file) = identity_file {
        //         SshAuth::PublicKey { identity_file }
        //     } else {
        //         SshAuth::Default
        //     };
        //
        //     let ssh_dest = SshDest::from_url(&url, ssh_auth);
        //
        //     ExecContext::ModelProbe {
        //         ssh_dest,
        //         model,
        //         filter,
        //         args,
        //     }
        // }
        // Command::Init { path } => ExecContext::ModelInit {
        //     path: path.canonicalize().expect("Error resolving path"),
        // },
        // Command::Remote {
        //     expr,
        //     command,
        //     model,
        // } => {
        //     let command = command.join(" ");
        //     ExecContext::RemoteExec {
        //         expr,
        //         command,
        //         model_path: model,
        //     }
        // }
        _ => {
            unimplemented!()
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
