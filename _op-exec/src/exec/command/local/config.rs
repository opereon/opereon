#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalConfig {
    runas_cmd: String,
    shell_cmd: String,
}

impl LocalConfig {
    pub fn runas_cmd(&self) -> &str {
        &self.runas_cmd
    }

    pub fn shell_cmd(&self) -> &str {
        &self.shell_cmd
    }
}

impl Default for LocalConfig {
    fn default() -> Self {
        LocalConfig {
            runas_cmd: "/bin/sudo".into(),
            shell_cmd: "/bin/bash".into(),
        }
    }
}
