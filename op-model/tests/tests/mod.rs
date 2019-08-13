use super::*;

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

mod defs;
mod model;
mod config;
mod git;