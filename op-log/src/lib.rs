extern crate colored;
extern crate slog;

use crate::colored::Colorize;
use slog::{Drain, Never};
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Mutex;

mod logger;

pub use logger::*;


pub fn build_file_drain<P: AsRef<Path>>(
    log_path: P,
    level: slog::Level,
) -> impl Drain<Ok = (), Err = Never> {
    if let Some(log_dir) = log_path.as_ref().parent() {
        std::fs::create_dir_all(log_dir).expect("Cannot create log dir");
    }

    let mut open_opts = OpenOptions::new();

    open_opts.create(true).append(true);

    let log_file = open_opts.open(log_path).expect("Cannot open log file");

    let drain = slog_bunyan::default(log_file);

    //    let decorator = slog_term::PlainSyncDecorator::new(log_file.try_clone().unwrap());
    //    let drain = slog_term::FullFormat::new(decorator).build();
    let drain = slog::LevelFilter::new(Mutex::new(drain), level);
    drain.fuse()
}

/// Logger for logging messages directly to user.
/// Each message is also logged to provided `slog::Logger`
pub struct CliLogger {
    verbosity: usize,
    logger: slog::Logger
}

impl CliLogger {
    pub fn new(verbosity: usize, logger: slog::Logger) -> CliLogger {
        CliLogger {
            verbosity,
            logger,
        }
    }
}

impl Logger for CliLogger {
    fn log(&mut self, record: &Record) {
        if record.verbosity() > self.verbosity {
            return;
        }
        let prefix = match record.level() {
            Level::Error => "Error:".bright_red(),
            Level::Warn => "Warn:".yellow(),
            Level::Info => "Info:".blue(),
        };
        slog::info!(self.logger, "CLI OUT: {} {}", record.level(), record.msg());

        println!("{} {}", prefix, record.msg())
    }
}