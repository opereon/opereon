#[macro_use]
extern crate  serde_derive;

use colored::Colorize;
use slog::{Drain, Never};
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Mutex;

mod logger;

pub use logger::*;
use tracing::subscriber::DefaultGuard;
use tracing_subscriber::layer::{SubscriberExt, Context};
use tracing_subscriber::{Layer, registry};
use tracing::{Subscriber, Event, Id, Metadata, Span};
use tracing::span::Attributes;
use tracing_subscriber::field::RecordFields;
use tracing::field::{Visit, Field};
use std::fmt::Debug;
use crate::term::TermLayer;
use crate::file::FileLayer;
use crate::config::LogConfig;

mod term;
mod file;
pub mod config;

#[derive(Copy, Clone, Debug, Hash, Eq, Serialize, Deserialize,)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
    Critical = 5,
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Level::Trace => {f.write_str("Trace")},
            Level::Debug => {f.write_str("Debug")},
            Level::Info => {f.write_str("Info")},
            Level::Warn => {f.write_str("Warn")},
            Level::Error => {f.write_str("Error")},
            Level::Critical => {f.write_str("Critical")},
        }
    }
}

impl PartialEq for Level {
    #[inline]
    fn eq(&self, other: &Level) -> bool {
        *self as usize == *other as usize
    }
}

impl Into<slog::Level> for Level {
    fn into(self) -> slog::Level {
        match self {
            Level::Trace => slog::Level::Trace,
            Level::Debug => slog::Level::Debug,
            Level::Info => slog::Level::Info,
            Level::Warn => slog::Level::Warning,
            Level::Error => slog::Level::Error,
            Level::Critical => slog::Level::Critical,
        }
    }
}

impl From<tracing::Level> for Level {
    fn from(l: tracing::Level) -> Self {
        unimplemented!()
    }
}

pub fn init_tracing(verbosity: u8, cfg: &LogConfig) {
    let mut file_layer = FileLayer::new(cfg.level(), cfg.log_path());

    file_layer.init();

    let subscriber = tracing_subscriber::fmt()
        .finish()
        .with(TermLayer::new(verbosity))
        .with(file_layer);


    tracing::subscriber::set_global_default(subscriber).unwrap()
}


// /// Logger for logging messages directly to user.
// /// Each message is also logged to provided `slog::Logger`
// pub struct CliLogger {
//     verbosity: usize,
//     logger: slog::Logger,
// }

// impl CliLogger {
//     pub fn new(verbosity: usize, logger: slog::Logger) -> CliLogger {
//         CliLogger {
//             verbosity,
//             logger,
//         }
//     }
// }

// impl Logger for CliLogger {
//     fn log(&mut self, record: &Record) {
//         if record.verbosity() > self.verbosity {
//             return;
//         }
//         let prefix = match record.level() {
//             Level::Error => "Error:".bright_red(),
//             Level::Warn => "Warn:".yellow(),
//             Level::Info => "Info:".blue(),
//         };
//         slog::info!(self.logger, "CLI OUT: {} {}", record.level(), record.msg());
//
//         println!("{} {}", prefix, record.msg())
//     }
// }