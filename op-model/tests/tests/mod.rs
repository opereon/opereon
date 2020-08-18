use super::*;
use op_rev::Oid;

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

pub fn initial_commit(path: &Path) -> Oid {
    op_test_helpers::initial_commit(path).into()
}

//FIXME (jc) some tests are currently broken since op-rev crate was introduced.

// mod config;
mod defs;
// mod git;
// mod load_file;
mod model;
