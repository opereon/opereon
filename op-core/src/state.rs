use crate::config::ConfigRef;

pub struct CoreState {
    config: ConfigRef,
}

impl CoreState {
    pub fn new(config: ConfigRef) -> Self {
        CoreState { config }
    }

    pub fn config(&self) -> &ConfigRef {
        &self.config
    }
}
