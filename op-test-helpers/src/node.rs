use kg_tree::{NodeRef, Value};

/// assert that `NodesSet::One` and returns single `NodeRef`.
#[macro_export]
macro_rules! assert_one {
    ($node_set:expr) => {{
        match $node_set {
            kg_tree::opath::NodeSet::One(node) => node,
            got => panic!("Expected single node, got: {:?}", got),
        }
    }};
}

/// Helper trait for `NodeRef` assertions
pub trait NodeRefExt {
    fn as_int_ext(&self) -> i64;
    fn as_float_ext(&self) -> f64;
    fn as_bool_ext(&self) -> bool;
    fn as_string_ext(&self) -> String;
    fn as_array_ext(&self) -> Vec<NodeRef>;
    fn is_empty_ext(&self) -> bool;
    fn get_key(&self, key: &str) -> NodeRef;
    fn get_idx(&self, idx: usize) -> NodeRef;
}

impl NodeRefExt for NodeRef {
    fn as_int_ext(&self) -> i64 {
        assert!(self.is_integer());
        self.as_integer().unwrap()
    }

    fn as_float_ext(&self) -> f64 {
        assert!(self.is_float());
        self.as_float()
    }

    fn as_bool_ext(&self) -> bool {
        assert!(self.is_boolean());
        self.as_boolean()
    }

    fn as_string_ext(&self) -> String {
        assert!(self.is_string());
        self.as_string()
    }

    fn as_array_ext(&self) -> Vec<NodeRef> {
        assert!(self.is_array());
        match self.data().value() {
            Value::Array(elems) => elems.clone(),
            _ => unreachable!(),
        }
    }

    fn is_empty_ext(&self) -> bool {
        self.data()
            .children_count()
            .expect("cannot get children count")
            == 0
    }

    fn get_key(&self, key: &str) -> NodeRef {
        self.get_child_key(key)
            .expect(&format!("key not found: '{}'", key))
    }

    fn get_idx(&self, idx: usize) -> NodeRef {
        self.get_child_index(idx)
            .expect(&format!("index not found: '{}'", idx))
    }
}
