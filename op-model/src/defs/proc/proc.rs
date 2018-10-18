use super::*;

use std::str::FromStr;


#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all="kebab-case")]
pub enum ProcKind {
    Exec,
    Check,
    Update,
}

impl FromStr for ProcKind {
    type Err = DefsParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "exec" => Ok(ProcKind::Exec),
            "update" => Ok(ProcKind::Update),
            "check" => Ok(ProcKind::Check),
            _ => perr!("unknown proc kind"), //FIXME (jc)
        }
    }
}

impl Default for ProcKind {
    fn default() -> Self {
        ProcKind::Exec
    }
}


#[derive(Debug, Clone, Serialize)]
pub struct ProcDef {
    #[serde(flatten)]
    scoped: Scoped,
    kind: ProcKind,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    watches: Vec<Watch>,
    run: Run,
    id: String,
    label: String,
    path: PathBuf,
    dir: PathBuf,
}

impl ProcDef {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn kind(&self) -> ProcKind {
        self.kind
    }

    pub fn run(&self) -> &Run {
        &self.run
    }

    pub fn watches(&self) -> &[Watch] {
        &self.watches
    }
}

impl AsScoped for ProcDef {
    fn as_scoped(&self) -> &Scoped {
        &self.scoped
    }
}

impl Remappable for ProcDef {
    fn remap(&mut self, node_map: &NodeMap) {
        self.scoped.remap(node_map);
        self.run.remap(node_map);
    }
}

impl ModelDef for ProcDef {
    fn root(&self) -> &NodeRef {
        self.as_scoped().root()
    }

    fn node(&self) -> &NodeRef {
        self.as_scoped().node()
    }
}

impl ScopedModelDef for ProcDef {
    fn scope_def(&self) -> &ScopeDef {
        self.as_scoped().scope_def()
    }

    fn scope(&self) -> &Scope {
        self.as_scoped().scope()
    }

    fn scope_mut(&self) -> &ScopeMut {
        self.as_scoped().scope_mut()
    }
}

impl ParsedModelDef for ProcDef {
    fn parse(model: &Model, parent: &Scoped, node: &NodeRef) -> Result<Self, DefsParseError> {
        let mut p = ProcDef {
            scoped: Scoped::new(parent.root(), node, ScopeDef::parse(model, parent, node)?),
            kind: ProcKind::Exec,
            run: Run::new(),
            watches: Vec::new(),
            id: String::new(),
            label: String::new(),
            path: PathBuf::new(),
            dir: PathBuf::new(),
        };

        match *node.data().value() {
            Value::Object(ref props) => {
                if let Some(_) = props.get("when") {
                    p.kind = ProcKind::Exec;
                } else if let Some(n) = props.get("proc") {
                    p.kind = ProcKind::from_str(&n.data().as_string())?;
                } else {
                    return perr!("procedure must have defined 'proc' property");
                }

                if p.kind == ProcKind::Update {
                    if let Some(wn) = node.get_child_key("watch") {
                        match *wn.data().value() {
                            Value::Object(ref props) => {
                                for (k, v) in props.iter() {
                                    let w = Watch::parse(k.as_ref(), &v.data().as_string())?;
                                    p.watches.push(w);
                                }
                            }
                            Value::Null => {}
                            _ => return perr!("watch definition must be an object"), //FIXME (jc)
                        }
                    }
                }

                p.run = Run::parse(model, &p.scoped, node)?;
            }
            _ => return perr!("procedure definition must be an object"), //FIXME (jc)
        }

        p.id = get_expr(&p, "@.id or @.@key");
        p.label = get_expr(&p, "@.label or @.id or @.@key");
        p.path = get_expr(&p, "@.@file_path_abs");
        p.dir = get_expr(&p, "@.@dir_abs");

        Ok(p)
    }
}
