use kg_template::parse::Config;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateConfig {
    kg: Config,
}

impl TemplateConfig {
    pub fn kg(&self) -> &Config {
        &self.kg
    }
}

impl Default for TemplateConfig {
    fn default() -> Self {
        TemplateConfig {
            kg: Config::default(),
        }
    }
}
