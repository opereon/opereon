use op_exec::command::config::CommandConfig;
use op_exec::rsync::RsyncConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecConfig {
    command: CommandConfig,
    rsync: RsyncConfig,
    // template: TemplateConfig,
}

impl ExecConfig {
    pub fn command(&self) -> &CommandConfig {
        &self.command
    }

    pub fn rsync(&self) -> &RsyncConfig {
        &self.rsync
    }

    // pub fn template(&self) -> &TemplateConfig {
    //     &self.template
    // }
}

impl Default for ExecConfig {
    fn default() -> Self {
        ExecConfig {
            command: CommandConfig::default(),
            rsync: RsyncConfig::default(),
            // template: TemplateConfig::default(),
        }
    }
}
