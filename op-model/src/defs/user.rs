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
    fn parse(_model: &Model, parent: &Scoped, node: &NodeRef) -> Result<Self, DefsParseError> {
        match *node.data().value() {
            Value::Object(ref props) => {
                perr_assert!(
                    props.contains_key("username"),
                    "user definition must have 'username' property"
                )?; //FIXME (jc)
            }
            _ => {
                perr!("user definition must be an object")?; //FIXME (jc)
            }
        }
        Ok(UserDef::new(parent.root().clone(), node.clone()))
    }
}
