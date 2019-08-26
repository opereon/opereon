use super::*;
use op_model::Sha1Hash;
use std::path::Path;
/// Creates `NodeRef`
macro_rules! node {
    () => {{
        node!("{}", "json")
    }};
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
        kg_tree::NodeRef::from_str($str.into(), $format.into()).unwrap_disp()
    }};
}

pub fn initial_commit(path: &Path) -> Sha1Hash {
    op_test_helpers::initial_commit(path).into()
}

mod config;
mod defs;
mod git;
mod load_file;
mod model;
