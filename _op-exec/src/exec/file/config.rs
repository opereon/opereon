use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileConfig {
    rsync: RsyncConfig,
}

impl FileConfig {
    pub fn rsync(&self) -> &RsyncConfig {
        &self.rsync
    }
}

impl Default for FileConfig {
    fn default() -> Self {
        FileConfig {
            rsync: RsyncConfig::default(),
        }
    }
}
