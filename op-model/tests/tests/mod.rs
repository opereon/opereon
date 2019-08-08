use super::*;

/// Creates `NodeRef` from json
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
    ($str:expr, $format:expr) => {{
        NodeRef::from_str($str.into(), $format.into()).unwrap_disp()
    }};
}

mod defs;
