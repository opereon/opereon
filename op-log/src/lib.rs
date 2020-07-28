#[macro_use]
extern crate serde_derive;

use crate::config::LogConfig;
use crate::file::FileLayer;
use crate::term::TermLayer;
use std::fmt::Debug;

use tracing_subscriber::layer::SubscriberExt;

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

impl Into<tracing::Level> for Level {
    fn into(self) -> tracing::Level {
        match self {
            Level::Trace => tracing::Level::TRACE,
            Level::Debug => tracing::Level::DEBUG,
            Level::Info => tracing::Level::INFO,
            Level::Warn => tracing::Level::WARN,
            Level::Error => tracing::Level::ERROR,
            Level::Critical => tracing::Level::ERROR,
        }
    }
}
pub fn init_tracing(verbosity: u8, cfg: &LogConfig) {
    let mut file_layer = FileLayer::new(cfg.level(), cfg.log_path());

    file_layer.init();

    let level: tracing::Level = cfg.level().into();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(level)
        .finish()
        .with(TermLayer::new(verbosity))
        .with(file_layer);

    tracing::subscriber::set_global_default(subscriber).unwrap()
}
