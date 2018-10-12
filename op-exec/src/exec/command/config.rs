use super::*;


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommandConfig {
    local: LocalConfig,
    ssh: SshConfig,
}

impl CommandConfig {
    pub fn local(&self) -> &LocalConfig {
        &self.local
    }

    pub fn ssh(&self) -> &SshConfig {
        &self.ssh
    }
}

impl Default for CommandConfig {
    fn default() -> Self {
        CommandConfig {
            local: LocalConfig::default(),
            ssh: SshConfig::default(),
        }
    }
}
