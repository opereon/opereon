#![feature(box_syntax, min_specialization, raw)]

#[cfg(test)]
#[macro_use]
extern crate indoc;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate kg_display_derive;
#[macro_use]
extern crate kg_diag_derive;
#[macro_use]
extern crate async_trait;

use std::path::{Path, PathBuf};

use kg_diag::*;
use kg_tree::diff::ChangeKind;

mod meta;
mod impls;

pub use self::meta::*;
pub use self::impls::*;
use std::thread;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

#[async_trait]
pub trait FileVersionManager: Send + std::fmt::Debug {
    async fn resolve(&mut self, rev_path: &RevPath) -> Result<Oid, BasicDiag>;

    async fn checkout(&mut self, rev_id: Oid) -> Result<RevInfo, BasicDiag>;

    async fn commit(&mut self, message: &str) -> Result<Oid, BasicDiag>;

    async fn get_file_diff(&mut self, old_rev_id: Oid, new_rev_id: Oid) -> Result<FileDiff, BasicDiag>;
}


pub async fn open_repository<P: AsRef<Path> + Into<PathBuf>>(repo_path: P) -> Result<Box<dyn FileVersionManager + Send>, BasicDiag> {
    let git = GitManager::open(repo_path).await?;
    Ok(Box::new(git))
}

pub async fn create_repository<P: AsRef<Path> + Into<PathBuf>>(repo_path: P) -> Result<Box<dyn FileVersionManager + Send>, BasicDiag> {
    let git = GitManager::create(repo_path).await?;
    Ok(Box::new(git))
}

fn spawn_blocking<T, F>(f: F) -> JoinHandle<T>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
{
    // TODO ws use threadpool? see https://docs.rs/tokio/0.2.21/tokio/runtime/struct.Handle.html#method.spawn_blocking

    tokio::task::spawn_blocking(|| {
        f()
    })
}
