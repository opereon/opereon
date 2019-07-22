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
    pub fn new(root: NodeRef, node: NodeRef) -> DefsParseResult<HostDef> {
        let mut h = HostDef {
            root,
            node,
            hostname: String::new(),
        };
        h.hostname = get_expr(&h, "fqdn or hostname")?;
        Ok(h)
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
    fn parse(_model: &Model, parent: &Scoped, node: &NodeRef) -> DefsParseResult<Self> {
        let kind = node.data().kind();
        match *node.data().value() {
            Value::Object(ref props) => {
                if !props.contains_key("hostname") {
                    return Err(DefsParseErrorDetail::HostMissingHostname.into());
                }

                if !props.contains_key("ssh_dest") {
                    return Err(DefsParseErrorDetail::HostMissingSshDest.into());
                }
            }
            _ => {
                return Err(DefsParseErrorDetail::HostNonObject { kind }.into());
            }
        }
        Ok(HostDef::new(parent.root().clone(), node.clone())?)
    }
}
