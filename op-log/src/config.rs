use crate::Level;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    level: Level,
    log_path: PathBuf,
}

impl LogConfig {
    pub fn level(&self) -> &Level {
        &self.level
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        LogConfig {
            level: Level::Info,
            log_path: PathBuf::from("/var/log/opereon/opereon.log"),
        }
    }
}
