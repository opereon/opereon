use std::borrow::Cow;

use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct Run {
    steps: Vec<Step>,
}

impl Run {
    pub(crate) fn new() -> Run {
        Run { steps: Vec::new() }
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }
}

impl ParsedModelDef for Run {
    fn parse(model: &Model, parent: &Scoped, node: &NodeRef) -> DefsResult<Self> {
        let mut run = Run { steps: Vec::new() };

        if let Some(rn) = node.get_child_key("run") {
            let kind = rn.data().kind();
            match *rn.data().value() {
                Value::Array(ref elems) => {
                    for (idx, v) in elems.iter().enumerate() {
                        let mut step = Step::parse(model, parent, v).map_err(|err| {
                            DefsErrorDetail::StepParseErr {
                                step: idx.to_string(),
                                err: Box::new(err),
                            }
                        })?;
                        step.index = run.steps.len();
                        run.steps.push(step);
                    }
                }
                Value::Object(ref props) => {
                    for (idx, (k, v)) in props.iter().enumerate() {
                        let mut step = Step::parse(model, parent, v).map_err(|err| {
                            DefsErrorDetail::StepParseErr {
                                step: format!("{} ({})", idx, k),
                                err: Box::new(err),
                            }
                        })?;
                        step.index = run.steps.len();
                        run.steps.push(step);
                    }
                }
                Value::Null => {}
                _ => {
                    return Err(DefsErrorDetail::UnexpectedPropType {
                        kind,
                        expected: vec![Kind::Array, Kind::Object],
                    }
                    .into())
                }
            }
        }
        Ok(run)
    }
}

impl Remappable for Run {
    fn remap(&mut self, node_map: &NodeMap) {
        self.steps.iter_mut().for_each(|s| s.remap(node_map));
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Step {
    index: usize,
    hosts: Option<Opath>,
    tasks: Vec<TaskDef>,
}

impl Step {
    pub fn resolve_hosts<'a>(
        &self,
        model: &'a Model,
        proc: &ProcDef,
    ) -> DefsResult<Vec<Cow<'a, HostDef>>> {
        self.hosts().map_or(
            Ok(model.hosts().iter().map(|h| Cow::Borrowed(h)).collect()),
            |hosts_expr| {
                let hs = hosts_expr
                    .apply_ext(proc.root(), proc.node(), proc.scope()?)
                    .map_err(|err| DefsErrorDetail::ExprErr { err: Box::new(err) })?;
                let mut res = Vec::with_capacity(hs.len());
                for h in hs.iter() {
                    let host: Cow<HostDef> = match model.get_host(h) {
                        Some(host) => Cow::Borrowed(host),
                        None => Cow::Owned(HostDef::parse(model, model.as_scoped(), h)?),
                    };
                    res.push(host);
                }
                Ok(res)
            },
        )
    }

    pub fn tasks(&self) -> &[TaskDef] {
        &self.tasks
    }

    pub fn hosts(&self) -> Option<&Opath> {
        self.hosts.as_ref()
    }

    pub fn index(&self) -> usize {
        self.index
    }
}

impl ParsedModelDef for Step {
    fn parse(model: &Model, parent: &Scoped, node: &NodeRef) -> DefsResult<Self> {
        if let Value::Object(ref props) = *node.data().value() {
            let hosts = if let Some(h) = props.get("hosts") {
                match ValueDef::parse(h)? {
                    ValueDef::Static(_n) => {
                        return Err(DefsErrorDetail::StepStaticHosts.into());
                    }
                    ValueDef::Resolvable(h) => Some(h),
                }
            } else {
                None
            };

            let tasks = if let Some(t) = props.get("tasks") {
                let kind = t.data().kind();
                match *t.data().value() {
                    Value::Array(ref elems) => elems
                        .iter()
                        .map(|t| TaskDef::parse(model, parent, t))
                        .collect::<Result<Vec<_>, _>>()?,
                    Value::Object(ref props) => props
                        .values()
                        .map(|t| TaskDef::parse(model, parent, t))
                        .collect::<Result<Vec<_>, _>>()?,
                    _ => {
                        return Err(DefsErrorDetail::UnexpectedPropType {
                            kind,
                            expected: vec![Kind::Array, Kind::Object],
                        }
                        .into())
                    }
                }
            } else {
                return Err(DefsErrorDetail::StepMissingTasks.into());
            };

            Ok(Step {
                index: 0,
                hosts,
                tasks,
            })
        } else {
            return Err(DefsErrorDetail::UnexpectedPropType {
                kind: node.data().kind(),
                expected: vec![Kind::Object],
            }
            .into());
        }
    }
}

impl Remappable for Step {
    fn remap(&mut self, node_map: &NodeMap) {
        self.tasks.iter_mut().for_each(|t| t.remap(node_map));
    }
}
