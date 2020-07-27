use std::str::FromStr;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProcKind {
    Exec,
    Check,
    Update,
    Probe,
}

impl FromStr for ProcKind {
    type Err = DefsErrorDetail;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "exec" => Ok(ProcKind::Exec),
            "update" => Ok(ProcKind::Update),
            "check" => Ok(ProcKind::Check),
            "probe" => Ok(ProcKind::Probe),
            unknown => Err(DefsErrorDetail::UnknownProcKind {
                value: unknown.to_string(),
            }),
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
    model_watches: Vec<ModelWatch>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    file_watches: Vec<FileWatch>,
    run: RunDef,
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

    pub fn run(&self) -> &RunDef {
        &self.run
    }

    pub fn model_watches(&self) -> &[ModelWatch] {
        &self.model_watches
    }

    pub fn file_watches(&self) -> &[FileWatch] {
        &self.file_watches
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

    fn scope(&self) -> DefsResult<&Scope> {
        self.as_scoped().scope()
    }

    fn scope_mut(&self) -> DefsResult<&ScopeMut> {
        self.as_scoped().scope_mut()
    }
}

impl ParsedModelDef for ProcDef {
    fn parse(model: &Model, parent: &Scoped, node: &NodeRef) -> DefsResult<Self> {
        let mut p = ProcDef {
            scoped: Scoped::new(parent.root(), node, ScopeDef::parse(model, parent, node)?),
            kind: ProcKind::Exec,
            run: RunDef::new(),
            model_watches: Vec::new(),
            file_watches: Vec::new(),
            id: String::new(),
            label: String::new(),
            path: PathBuf::new(),
            dir: PathBuf::new(),
        };

        match *node.data().value() {
            Value::Object(ref props) => {
                if props.get("when").is_some() {
                    p.kind = ProcKind::Exec;
                } else if let Some(n) = props.get("proc") {
                    p.kind = ProcKind::from_str(&n.data().as_string())?;
                } else {
                    return Err(DefsErrorDetail::ProcMissingProc.into());
                }

                if p.kind == ProcKind::Update {
                    if let Some(wn) = node.get_child_key("watch") {
                        let kind = wn.data().kind();
                        match *wn.data().value() {
                            Value::Object(ref props) => {
                                for (k, v) in props.iter() {
                                    let w = ModelWatch::parse(k.as_ref(), &v.data().as_string())?;
                                    p.model_watches.push(w);
                                }
                            }
                            Value::Null => {}
                            _ => return Err(DefsErrorDetail::ProcWatchNonObject { kind }.into()),
                        }
                    }
                    if let Some(wn) = node.get_child_key("watch_file") {
                        let kind = wn.data().kind();
                        match *wn.data().value() {
                            Value::Object(ref props) => {
                                for (k, v) in props.iter() {
                                    let w = FileWatch::parse(k.as_ref(), &v.data().as_string())?;
                                    p.file_watches.push(w);
                                }
                            }
                            Value::Null => {}
                            _ => return Err(DefsErrorDetail::ProcWatchNonObject { kind }.into()),
                        }
                    }
                }

                p.run = RunDef::parse(model, &p.scoped, node)
                    .map_err_as_cause(|| DefsErrorDetail::RunParse)?;
            }
            _ => {
                return Err(DefsErrorDetail::UnexpectedPropType {
                    kind: node.data().kind(),
                    expected: vec![Kind::Object],
                }
                .into())
            }
        }

        p.id = get_expr(&p, "@.id or @.@key")?;
        p.label = get_expr(&p, "@.label or @.id or @.@key")?;
        p.path = get_expr(&p, "@.@file_path_abs")?;
        p.dir = get_expr(&p, "@.@dir_abs")?;

        Ok(p)
    }
}
