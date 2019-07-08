use std::str::FromStr;

use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct TaskDef {
    #[serde(flatten)]
    scoped: Scoped,
    kind: TaskKind,
    read_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    switch: Option<Switch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<TaskOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env: Option<TaskEnv>,
    id: String,
    label: String,
}

impl TaskDef {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn kind(&self) -> TaskKind {
        self.kind
    }

    pub fn read_only(&self) -> bool {
        self.read_only
    }

    pub fn switch(&self) -> Option<&Switch> {
        self.switch.as_ref()
    }

    pub fn output(&self) -> Option<&TaskOutput> {
        self.output.as_ref()
    }

    pub fn env(&self) -> Option<&TaskEnv> {
        self.env.as_ref()
    }
}

impl AsScoped for TaskDef {
    fn as_scoped(&self) -> &Scoped {
        &self.scoped
    }
}

impl Remappable for TaskDef {
    fn remap(&mut self, node_map: &NodeMap) {
        self.scoped.remap(node_map);
        if let Some(ref mut switch) = self.switch {
            switch.remap(node_map);
        }
    }
}

impl ModelDef for TaskDef {
    fn root(&self) -> &NodeRef {
        self.as_scoped().root()
    }

    fn node(&self) -> &NodeRef {
        self.as_scoped().node()
    }
}

impl ScopedModelDef for TaskDef {
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

impl ParsedModelDef for TaskDef {
    fn parse(model: &Model, parent: &Scoped, node: &NodeRef) -> Result<Self, DefsParseError> {
        let mut t = TaskDef {
            scoped: Scoped::new(parent.root(), node, ScopeDef::parse(model, parent, node)?),
            kind: TaskKind::Exec,
            read_only: false,
            switch: None,
            output: None,
            env: None,
            id: String::new(),
            label: String::new(),
        };

        match *node.data().value() {
            Value::Object(ref props) => {
                if let Some(n) = props.get("task") {
                    t.kind = TaskKind::from_str(&n.data().as_string())?;
                } else {
                    return perr!("task definition must have 'task' property"); //FIXME (jc)
                }

                if let Some(n) = props.get("ro") {
                    if n.as_boolean() {
                        t.read_only = true
                    }
                }

                if t.kind == TaskKind::Command || t.kind == TaskKind::Script {
                    if let Some(n) = props.get("env") {
                        t.env = Some(TaskEnv::parse(n)?);
                    }
                }

                if t.kind == TaskKind::Switch {
                    if let Some(s) = props.get("cases") {
                        t.switch = Some(Switch::parse(model, &t.scoped, s)?);
                    } else {
                        return perr!("switch task definition must have 'cases' property");
                    }
                }

                if let Some(n) = props.get("output") {
                    t.output = Some(TaskOutput::parse(n)?);
                }
            }
            _ => return perr!("task definition must be an object"), //FIXME (jc)
        }

        t.id = get_expr(&t, "@.id or (@.task + '-' + @.@key)");
        t.label = get_expr(&t, "@.label or @.id or (@.task + '-' + @.@key)");

        Ok(t)
    }
}


#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskKind {
    Exec,
    Switch,
    Template,
    Command,
    Script,
    FileCopy,
    FileCompare,
}

impl FromStr for TaskKind {
    type Err = DefsParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "exec" => Ok(TaskKind::Exec),
            "switch" => Ok(TaskKind::Switch),
            "template" => Ok(TaskKind::Template),
            "command" => Ok(TaskKind::Command),
            "script" => Ok(TaskKind::Script),
            "file-copy" => Ok(TaskKind::FileCopy),
            "file-compare" => Ok(TaskKind::FileCompare),
            _ => perr!("unknown task kind"), //FIXME (jc)
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputMode {
    Var(String),
    Expr(Opath),
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TaskOutput {
    #[serde(flatten)]
    mode: OutputMode,
    format: FileFormat,
}

impl TaskOutput {
    fn parse(node: &NodeRef) -> Result<TaskOutput, DefsParseError> {
        match *node.data().value() {
            Value::Object(_) => match kg_tree::serial::from_tree::<TaskOutput>(node) {
                Ok(out) => Ok(out),
                Err(_err) => Err(DefsParseError::Undef), //FIXME (jc)
            }
            Value::String(ref s) => {
                let format = FileFormat::from(s);
                Ok(TaskOutput {
                    format,
                    .. Default::default()
                })
            }
            _ => perr!("output definition must be an object or string"), //FIXME (jc)
        }
    }

    pub fn mode(&self) -> &OutputMode {
        &self.mode
    }

    pub fn format(&self) -> FileFormat {
        self.format
    }

    pub fn apply(&self, root: &NodeRef, current: &NodeRef, scope: &ScopeMut, output: NodeSet) {
        match self.mode {
            OutputMode::Var(ref name) => scope.set_var(name.into(), output),
            OutputMode::Expr(ref expr) => {
                let scope = ScopeMut::child(scope.clone().into());
                scope.set_var("$output".into(), output);
                expr.apply_ext(root, current, &scope);
            },
        }
    }
}

impl Default for TaskOutput {
    fn default() -> Self {
        TaskOutput {
            mode: OutputMode::Var("output".into()),
            format: FileFormat::Yaml,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "value")]
pub enum TaskEnv {
    Map(LinkedHashMap<String, Opath>),
    List(Vec<Opath>),
}

impl TaskEnv {
    pub fn parse(n: &NodeRef) -> Result<TaskEnv, DefsParseError> {
        let env = match *n.data().value() {
            Value::Object(ref props) => {
                let mut envs = LinkedHashMap::with_capacity(props.len());

                for (k, node) in props.iter() {
                    let expr: Opath = serial::from_tree(node)?;
                    envs.insert(k.to_string(), expr);
                }
                TaskEnv::Map(envs)
            }
            Value::Array(ref elems) => {
                let mut envs = Vec::with_capacity(elems.len());

                for node in elems.iter() {
                    let expr: Opath = serial::from_tree(node)?;
                    envs.push( expr)
                }
                TaskEnv::List(envs)
            }
            Value::String(ref key) => TaskEnv::List(vec![Opath::parse_opt_delims(&key, "${", "}")?]),
            _ => return perr!("Unexpected property type"), //FIXME (jc)
        };
        Ok(env)
    }
}


#[derive(Debug, Clone, Serialize)]
pub struct Switch {
    cases: Vec<Case>,
}

impl Switch {
    pub fn cases(&self) -> &[Case] {
        &self.cases
    }
}

impl ParsedModelDef for Switch {
    fn parse(model: &Model, parent: &Scoped, node: &NodeRef) -> Result<Self, DefsParseError> {
        let mut s = Switch {
            cases: Vec::new(),
        };

        if let Value::Array(ref elems) = *node.data().value() {
            for e in elems.iter() {
                let case = Case::parse(model, parent, e)?;
                s.cases.push(case);
            }
        } else {
            return perr!("switch definition must be an array");
        }

        Ok(s)
    }
}

impl Remappable for Switch {
    fn remap(&mut self, node_map: &NodeMap) {
        self.cases.iter_mut().for_each(|c| c.remap(node_map));
    }
}


#[derive(Debug, Clone, Serialize)]
pub struct Case {
    when: Opath,
    proc: ProcDef,
}

impl Case {
    pub fn when(&self) -> &Opath {
        &self.when
    }

    pub fn proc(&self) -> &ProcDef {
        &self.proc
    }
}

impl ParsedModelDef for Case {
    fn parse(model: &Model, parent: &Scoped, node: &NodeRef) -> Result<Self, DefsParseError> {
        if let Value::Object(ref props) = *node.data().value() {
            let when = if let Some(n) = props.get("when") {
                match ValueDef::parse(n)? {
                    ValueDef::Resolvable(e) => {
                        e
                    }
                    _ => return perr!("'when' property must be a dynamic expression in switch case definition"),
                }
            } else {
                return perr!("switch case expression must have 'when' property");
            };

            Ok(Case {
                when,
                proc: ProcDef::parse(model, parent, node)?,
            })
        } else {
            return perr!("switch case definition must be an object");
        }
    }
}

impl Remappable for Case {
    fn remap(&mut self, node_map: &NodeMap) {
        self.proc.remap(node_map);
    }
}
