#[macro_use]
extern crate structopt;

extern crate futures;
extern crate uuid;

extern crate serde;
extern crate serde_derive;
extern crate serde_json;
extern crate serde_yaml;
extern crate toml;

extern crate actix;

extern crate kg_diag;
extern crate kg_tree;
extern crate kg_utils;
extern crate op_exec;

#[macro_use]
extern crate slog;
extern crate core;
extern crate slog_async;
extern crate slog_term;

use slog::Drain;
use slog::FnValue;

use structopt::StructOpt;

use op_exec::OutcomeFuture;
use op_exec::{ConfigRef, Context as ExecContext, EngineRef, ModelPath};

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
    let Opts {
        config_file_path,
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

            ExecContext::ModelStore(model_path)
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
            name,
            args,
        } => {
            make_model_path_absolute(&mut model);

            ExecContext::ModelProbe {
                model,
                name,
                args,
            }
        }
    };

    actix::System::run(move || {
        local_run(config, cmd, disp_format);
    });
}
