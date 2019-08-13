use super::*;
use git2::Repository;
use op_model::Sha1Hash;
use std::path::Path;

/// Creates `NodeRef`
macro_rules! node {
    ($json:expr) => {{
        NodeRef::from_json($json).unwrap_disp()
    }};
    ($str:expr, toml) => {{
        node!($str, "toml")
    }};
    ($str:expr, yaml) => {{
        node!($str, "yaml")
    }};
    ($str:expr, json) => {{
        node!($str, "json")
    }};
    ($str:expr, $format:expr) => {{
        NodeRef::from_str($str.into(), $format.into()).unwrap_disp()
    }};
}

pub fn init_repo<P: AsRef<Path>>(path: P) -> Repository {
    Repository::init(path).expect("Cannot init git repository!")
}

pub fn initial_commit(path: &Path) -> Sha1Hash {
    let repo = Repository::open(path).unwrap();
    let sig = repo.signature().unwrap();

    let tree_id = {
        let mut index = repo.index().unwrap();
        index
            .add_all(&["*"], git2::IndexAddOption::default(), None)
            .unwrap();
        index.write_tree().unwrap()
    };

    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .unwrap()
        .into()
}

mod config;
mod defs;
mod git;
mod model;
