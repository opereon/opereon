#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RsyncConfig {
    rsync_cmd: String,
}

impl RsyncConfig {
    pub fn rsync_cmd(&self) -> &str {
        &self.rsync_cmd
    }
}

impl Default for RsyncConfig {
    fn default() -> Self {
        RsyncConfig {
            rsync_cmd: "/bin/rsync".into(),
        }
    }
}
