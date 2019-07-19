use super::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct ArgumentSet(Vec<ValueDef>);

impl ArgumentSet {
    pub fn new(node_set: &NodeSet, root: &NodeRef) -> ArgumentSet {
        ArgumentSet(
            node_set
                .iter()
                .map(|n| {
                    if n.data().is_root() && !n.is_ref_eq(root) {
                        ValueDef::Static(n.clone())
                    } else {
                        ValueDef::Resolvable(n.path())
                    }
                })
                .collect(),
        )
    }

    pub fn resolve(&self, root: &NodeRef, current: &NodeRef, scope: &Scope) -> NodeSet {
        let mut n = Vec::new();
        for v in self.0.iter() {
            n.push(v.resolve(root, current, scope).into_one().unwrap());
        }
        n.into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Arguments(LinkedHashMap<Symbol, ArgumentSet>);

impl Arguments {
    pub fn new() -> Arguments {
        Arguments(LinkedHashMap::new())
    }

    pub fn set_arg(&mut self, name: Symbol, value: ArgumentSet) {
        self.0.insert(name, value);
    }

    pub fn resolve(&self, root: &NodeRef, current: &NodeRef, scope: &ScopeMut) {
        for (k, v) in self.0.iter() {
            let n = v.resolve(root, current, scope);
            scope.set_var(k.clone(), n);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Default for Arguments {
    fn default() -> Self {
        Arguments::new()
    }
}
pub struct ArgumentsBuilder {
    root: NodeRef,
    args: Arguments,
}

impl ArgumentsBuilder {
    pub fn new(root: &NodeRef) -> ArgumentsBuilder {
        ArgumentsBuilder {
            root: root.clone(),
            args: Arguments::new(),
        }
    }

    pub fn set_arg(&mut self, name: Symbol, value: &NodeSet) -> &mut ArgumentsBuilder {
        let set = ArgumentSet::new(value, &self.root);
        self.args.set_arg(name, set);
        self
    }

    pub fn build(self) -> Arguments {
        self.args
    }
}
