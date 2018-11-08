use super::*;


#[derive(Debug, Clone, Serialize)]
pub struct HostDef {
    #[serde(skip)]
    root: NodeRef,
    #[serde(skip)]
    node: NodeRef,
    hostname: String,
}

impl HostDef {
    pub fn new(root: NodeRef, node: NodeRef) -> HostDef {
        let mut h = HostDef {
            root,
            node,
            hostname: String::new(),
        };
        h.hostname = get_expr(&h, "fqdn or hostname");
        h
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }
}

impl Remappable for HostDef {
    fn remap(&mut self, node_map: &NodeMap) {
        self.root = node_map.get(&self.root.data_ptr()).unwrap().clone();
        self.node = node_map.get(&self.node.data_ptr()).unwrap().clone();
    }
}

impl ModelDef for HostDef {
    fn root(&self) -> &NodeRef {
        &self.root
    }

    fn node(&self) -> &NodeRef {
        &self.node
    }
}

impl ParsedModelDef for HostDef {
    fn parse(_model: &Model, parent: &Scoped, node: &NodeRef) -> Result<Self, DefsParseError> {
        match *node.data().value() {
            Value::Object(ref props) => {
                perr_assert!(props.contains_key("hostname"), "host definition must contain 'hostname' property")?;
                perr_assert!(props.contains_key("ssh_dest"), "host definition must contain 'ssh_dest' property")?;
            }
            _ => {
                perr!("host definition must be an object")?;
            }
        }
        Ok(HostDef::new(parent.root().clone(), node.clone()))
    }
}
