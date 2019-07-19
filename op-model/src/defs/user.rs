use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct UserDef {
    #[serde(skip)]
    root: NodeRef,
    #[serde(skip)]
    node: NodeRef,
    username: String,
}

impl UserDef {
    pub fn new(root: NodeRef, node: NodeRef) -> UserDef {
        let mut u = UserDef {
            root,
            node,
            username: String::new(),
        };
        u.username = get_expr(&u, "username");
        u
    }

    pub fn username(&self) -> &str {
        &self.username
    }
}

impl Remappable for UserDef {
    fn remap(&mut self, node_map: &NodeMap) {
        self.root = node_map.get(&self.root.data_ptr()).unwrap().clone();
        self.node = node_map.get(&self.node.data_ptr()).unwrap().clone();
    }
}

impl ModelDef for UserDef {
    fn root(&self) -> &NodeRef {
        &self.root
    }

    fn node(&self) -> &NodeRef {
        &self.node
    }
}

impl ParsedModelDef for UserDef {
    fn parse(_model: &Model, parent: &Scoped, node: &NodeRef) -> DefsParseResult<Self> {
        match *node.data().value() {
            Value::Object(ref props) => {
                if !props.contains_key("username") {
                    return Err(DefsParseErrorDetail::UserMissingUsername.into())
                }
            }
            _ => {
                return Err(DefsParseErrorDetail::UserNonObject {kind: node.data().kind()}.into());
            }
        }
        Ok(UserDef::new(parent.root().clone(), node.clone()))
    }
}
