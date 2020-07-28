#[macro_use]
extern crate serde_derive;

use colored::Colorize;
use slog::{Drain, Never};
use std::fs::OpenOptions;
use std::path::Path;
use std::sync::Mutex;

use crate::config::LogConfig;
use crate::file::FileLayer;
use crate::term::TermLayer;
use std::fmt::Debug;
use tracing::field::{Field, Visit};
use tracing::span::Attributes;
use tracing::subscriber::DefaultGuard;
use tracing::{Event, Id, Metadata, Span, Subscriber};
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{registry, Layer};

pub mod config;
mod file;
mod term;

#[derive(Copy, Clone, Debug, Hash, Eq, Serialize, Deserialize)]
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
            Level::Trace => f.write_str("Trace"),
            Level::Debug => f.write_str("Debug"),
            Level::Info => f.write_str("Info"),
            Level::Warn => f.write_str("Warn"),
            Level::Error => f.write_str("Error"),
            Level::Critical => f.write_str("Critical"),
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

pub fn init_tracing(verbosity: u8, cfg: &LogConfig) {
    let mut file_layer = FileLayer::new(cfg.level(), cfg.log_path());

    file_layer.init();

    let subscriber = tracing_subscriber::fmt()
        .finish()
        .with(TermLayer::new(verbosity))
        .with(file_layer);

    tracing::subscriber::set_global_default(subscriber).unwrap()
}
