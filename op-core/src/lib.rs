#![feature(specialization)]

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate kg_diag_derive;

#[macro_use]
extern crate kg_display_derive;
#[macro_use]
extern crate slog;

use crate::config::{ConfigRef, ModelConfig};
use crate::services::model_manager::ModelManager;
use op_engine::engine::Service;
use std::path::PathBuf;

mod ops;
mod services;
mod utils;

pub mod config;
pub mod context;
pub mod outcome;

pub async fn init_services(
    repo_path: PathBuf,
    config: ConfigRef,
    logger: slog::Logger,
) -> Vec<Service> {
    let model_manager = ModelManager::new(repo_path, config.model().clone(), logger);

    vec![Box::new(model_manager)]
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
