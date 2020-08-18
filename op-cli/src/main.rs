extern crate structopt;

extern crate tracing;

use op_core::*;
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, Utc};

use structopt::StructOpt;
use url::Url;

use display::DisplayFormat;

use kg_diag::BasicDiag;
use op_rev::RevPath;
use options::*;

use op_core::config::ConfigRef;
use op_core::context::Context as ExecContext;
use op_core::state::CoreState;
use op_exec::command::ssh::{SshAuth, SshDest};
use op_engine::EngineRef;

mod display;
mod options;

pub static SHORT_VERSION: &str = env!("OP_SHORT_VERSION");
pub static LONG_VERSION: &str = env!("OP_LONG_VERSION");
pub static TIMESTAMP: &str = env!("OP_TIMESTAMP");

fn make_path_absolute(path: &Path) -> PathBuf {
    path.canonicalize().unwrap()
}

/// start engine and execute provided operation. Returns exit code
fn local_run(
    current_dir: PathBuf,
    config: ConfigRef,
    ctx: ExecContext,
    disp_format: DisplayFormat,
    verbosity: u8,
) -> Result<u32, BasicDiag> {
    op_log::init_tracing(verbosity, config.log());

    let mut rt = EngineRef::<()>::build_runtime();

    let out_res = rt.block_on(async {
        let services = init_services(current_dir, config.clone()).await?;
        let state = CoreState::new(config);

        let engine = EngineRef::new(services, state);

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
        res.unwrap()
    });

    let outcome = out_res?;

    display::display_outcome(&outcome, disp_format);
    Ok(0)
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
            model,
            format,
        } => {
            disp_format = format;
            ExecContext::ModelQuery { model, expr }
        }
        Command::Test { format, model } => {
            disp_format = format;
            ExecContext::ModelTest { model }
        }
        Command::Diff {
            format,
            source,
            target,
        } => {
            // FIXME fails when id provided instead of path (because of canonicalize)
            disp_format = format;

            ExecContext::ModelDiff {
                prev_model: source,
                next_model: target,
            }
        }
        Command::Update {
            format,
            source,
            target,
            dry_run,
        } => {
            disp_format = format;
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
            model,
            filter,
            dry_run,
        } => ExecContext::ModelCheck {
            model,
            filter,
            dry_run,
        },
        Command::Probe {
            model,
            url,
            password,
            identity_file,
            filter,
            args,
        } => {
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
