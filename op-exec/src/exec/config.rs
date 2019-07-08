use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecConfig {
    command: CommandConfig,
    file: FileConfig,
    template: TemplateConfig,
}

impl ExecConfig {
    pub fn command(&self) -> &CommandConfig {
        &self.command
    }

    pub fn file(&self) -> &FileConfig {
        &self.file
    }

    pub fn template(&self) -> &TemplateConfig {
        &self.template
    }
}

impl Default for ExecConfig {
    fn default() -> Self {
        ExecConfig {
            command: CommandConfig::default(),
            file: FileConfig::default(),
            template: TemplateConfig::default(),
        }
    }
}
