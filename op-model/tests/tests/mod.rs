use super::*;

/// Creates `NodeRef` from json
macro_rules! node {
    ($json:expr) => {{
        NodeRef::from_json($json).unwrap_disp()
    }};
}

mod defs;
