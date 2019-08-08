use serde::{de, ser};

use super::*;

#[derive(Debug, Clone)]
pub enum ValueDef {
    Static(NodeRef),
    Resolvable(Opath),
}

impl ValueDef {
    pub fn parse(node: &NodeRef) -> DefsResult<ValueDef> {
        match *node.data().value() {
            Value::String(ref s) => {
                let expr = s.trim();
                if expr.starts_with("${") && expr.ends_with('}') {
                    match Opath::parse(&expr[2..expr.len() - 1]) {
                        Ok(expr) => Ok(ValueDef::Resolvable(expr)),
                        Err(err) => {
                            Err(DefsErrorDetail::OpathParseErr { err: Box::new(err) }.into())
                        }
                    }
                } else {
                    Ok(ValueDef::Static(node.clone()))
                }
            }
            _ => Ok(ValueDef::Static(node.clone())),
        }
    }

    pub fn resolve(&self, root: &NodeRef, current: &NodeRef, scope: &Scope) -> DefsResult<NodeSet> {
        match *self {
            ValueDef::Static(ref n) => Ok(n.clone().into()),
            ValueDef::Resolvable(ref expr) => expr
                .apply_ext(root, current, scope)
                .map_err(|err| DefsErrorDetail::ExprErr { err: Box::new(err) }.into()),
        }
    }

    pub fn is_static(&self) -> bool {
        match *self {
            ValueDef::Static(_) => true,
            ValueDef::Resolvable(_) => false,
        }
    }
}

impl Remappable for ValueDef {
    fn remap(&mut self, node_map: &NodeMap) {
        match *self {
            ValueDef::Static(ref mut n) => {
                if let Some(nn) = node_map.get(&n.data_ptr()) {
                    *n = nn.clone();
                } else {
                    *n = n.deep_copy();
                }
            }
            ValueDef::Resolvable(..) => {}
        }
    }
}

impl ser::Serialize for ValueDef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        match *self {
            ValueDef::Static(ref n) => n.serialize(serializer),
            ValueDef::Resolvable(ref e) => e.serialize(serializer),
        }
    }
}

impl<'de> de::Deserialize<'de> for ValueDef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let n = NodeRef::deserialize(deserializer)?;
        ValueDef::parse(&n).map_err(|_err| de::Error::custom("opath parse error"))
        //FIXME (jc) error message
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeDef {
    #[serde(flatten)]
    values: LinkedHashMap<Symbol, ValueDef>,
}

impl ScopeDef {
    pub fn new() -> ScopeDef {
        ScopeDef {
            values: LinkedHashMap::new(),
        }
    }

    pub fn set_var_def(&mut self, name: Symbol, value: ValueDef) {
        self.values.insert(name, value);
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn get_var_def(&self, name: &str) -> Option<&ValueDef> {
        self.values.get(name)
    }

    pub fn resolve(&self, root: &NodeRef, current: &NodeRef, scope: &ScopeMut) -> DefsResult<()> {
        for (name, value) in self.values.iter() {
            let rval = value.resolve(root, current, &scope).map_err(|err| {
                DefsErrorDetail::ScopeValParseErr {
                    key: name.to_string(),
                    err: Box::new(err),
                }
            })?;
            scope.set_var(name.clone(), rval);
        }
        Ok(())
    }
}

impl Remappable for ScopeDef {
    fn remap(&mut self, node_map: &NodeMap) {
        self.values.iter_mut().for_each(|(_, v)| v.remap(node_map));
    }
}

impl ParsedModelDef for ScopeDef {
    fn parse(_model: &Model, _parent: &Scoped, node: &NodeRef) -> DefsResult<Self> {
        let mut scope = ScopeDef::new();

        if let Some(sn) = node.get_child_key("scope") {
            match *sn.data().value() {
                Value::Object(ref props) => {
                    for (k, v) in props.iter() {
                        let val = ValueDef::parse(v).map_err(|err| {
                            DefsErrorDetail::ScopeValParseErr {
                                key: k.to_string(),
                                err: Box::new(err),
                            }
                        })?;
                        scope.set_var_def(k.clone(), val);
                    }
                }
                Value::Null => {}
                _ => {
                    return Err(DefsErrorDetail::ScopeNonObject {
                        kind: sn.data().kind(),
                    }
                    .into())
                }
            }
        }
        Ok(scope)
    }
}
