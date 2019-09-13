#![feature(box_syntax, specialization, raw)]

#[cfg(test)]
#[macro_use]
extern crate indoc;

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate kg_display_derive;
#[macro_use]
extern crate kg_diag_derive;

use std::path::{Path, PathBuf};

use kg_diag::*;
use kg_tree::diff::ChangeKind;

mod meta;
mod impls;

pub use self::meta::*;
pub use self::impls::*;

pub trait FileVersionManager: std::fmt::Debug {
    fn resolve(&mut self, rev_path: &RevPath) -> Result<Oid, BasicDiag>;

    fn checkout(&mut self, rev_id: Oid) -> Result<RevInfo, BasicDiag>;

    fn commit(&mut self, message: &str) -> Result<Oid, BasicDiag>;

    fn get_file_diff(&mut self, old_rev_id: Oid, new_rev_id: Oid) -> Result<FileDiff, BasicDiag>;
}


pub fn open_repository<P: AsRef<Path> + Into<PathBuf>>(repo_path: P) -> Result<Box<dyn FileVersionManager>, BasicDiag> {
    let git = GitManager::open(repo_path)?;
    Ok(Box::new(git))
}

pub fn create_repository<P: AsRef<Path> + Into<PathBuf>>(repo_path: P) -> Result<Box<dyn FileVersionManager>, BasicDiag> {
    let git = GitManager::create(repo_path)?;
    Ok(Box::new(git))
}
