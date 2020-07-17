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

use crate::config::ConfigRef;
use crate::services::model_manager::ModelManager;
use op_engine::engine::Service;
use std::path::PathBuf;

mod ops;
mod services;
mod utils;

pub mod config;
pub mod context;
pub mod outcome;
pub mod state;

use kg_diag::BasicDiag;
use op_exec2::command::ssh::SshSessionCache;
pub use op_exec2::command::ssh::{SshAuth, SshDest};

pub async fn init_services(
    repo_path: PathBuf,
    config: ConfigRef,
    logger: slog::Logger,
) -> Result<Vec<Service>, BasicDiag> {
    let model_manager = ModelManager::new(repo_path, config.model().clone(), logger);
    let mut ssh_session_cache = SshSessionCache::new(config.exec().command().ssh().clone());
    ssh_session_cache.init().await?;

    Ok(vec![Box::new(model_manager), Box::new(ssh_session_cache)])
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
