#![feature(min_specialization)]

#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate kg_diag_derive;

#[macro_use]
extern crate kg_display_derive;

use chrono::prelude::*;

use kg_diag::*;
use kg_diag::io::fs;
use kg_tree::*;
use kg_tree::opath::Opath;
use kg_tree::serial::{from_tree, to_tree};
use op_rev::*;
use op_model::*;
use op_engine::engine::Service;
use op_exec::command::ssh::{SshSessionCache, SshAuth, SshDest};

#[macro_use]
extern crate tracing;

use crate::config::ConfigRef;
use crate::services::model_manager::ModelManager;

use std::path::{Path, PathBuf};

mod ops;
mod services;
mod utils;
mod proto;

pub mod config;
pub mod context;
pub mod outcome;
pub mod state;


pub async fn init_services(
    repo_path: PathBuf,
    config: ConfigRef,
) -> Result<Vec<Service>, BasicDiag> {
    let model_manager = ModelManager::new(repo_path, config.model().clone());
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
