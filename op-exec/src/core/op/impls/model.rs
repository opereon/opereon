use super::*;

use regex::Regex;

use kg_tree::diff::Diff;


#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiffMethod {
    Minimal,
    Full,
}

impl std::str::FromStr for DiffMethod {
    type Err = String; //FIXME (jc) use some better error implementation

    fn from_str(s: &str) -> Result<DiffMethod, Self::Err> {
        match s {
            "minimal" => Ok(DiffMethod::Minimal),
            "full" => Ok(DiffMethod::Full),
            _ => Err("unknown diff method".to_string())
        }
    }
}


#[derive(Debug)]
pub struct ModelListOperation {
    operation: OperationRef,
    engine: EngineRef,
}

impl ModelListOperation {
    pub fn new(operation: OperationRef, engine: EngineRef) -> ModelListOperation {
        ModelListOperation {
            operation,
            engine,
        }
    }
}

impl Future for ModelListOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let list = to_tree(&self.engine.read().model_manager().list().unwrap()).unwrap();
        Ok(Async::Ready(Outcome::NodeSet(list.into())))
    }
}

impl OperationImpl for ModelListOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}


#[derive(Debug)]
pub struct ModelStoreOperation {
    operation: OperationRef,
    engine: EngineRef,
    path: PathBuf,
}

impl ModelStoreOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, path: PathBuf) -> ModelStoreOperation {
        ModelStoreOperation {
            operation,
            engine,
            path,
        }
    }
}

impl Future for ModelStoreOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut e = self.engine.write();
        let metadata = Metadata::new(Sha1Hash::default(), self.path.clone(), User::current(), Utc::now());
        let m = e.model_manager_mut().store(metadata, &self.path)?;
        e.model_manager_mut().set_current(m)?;
        Ok(Async::Ready(Outcome::Empty))
    }
}

impl OperationImpl for ModelStoreOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        self.path = self.path.canonicalize()?;
        Ok(())
    }
}


#[derive(Debug)]
pub struct ModelQueryOperation {
    operation: OperationRef,
    engine: EngineRef,
    model_path: ModelPath,
    expr: String,
}

impl ModelQueryOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, model_path: ModelPath, expr: String) -> ModelQueryOperation {
        ModelQueryOperation {
            operation,
            engine,
            model_path,
            expr,
        }
    }
}

impl Future for ModelQueryOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut e = self.engine.write();
        let m = e.model_manager_mut().resolve(&self.model_path)?;
        match Opath::parse(&self.expr) {
            Ok(expr) => {
                println!("{}", expr);
                let model_id = m.lock().metadata().id();
                let res = {
                    let m = m.lock();
                    kg_tree::set_base_path(m.metadata().path());
                    let scope = m.scope();
                    expr.apply_ext(m.root(), m.root(), &scope)
                };

                Ok(Async::Ready(Outcome::NodeSet(res.into())))
            }
            Err(err) => {
                eprintln!("{}", err); //FIXME (jc) error handling
                Err(RuntimeError::Custom)
            }
        }
    }
}

impl OperationImpl for ModelQueryOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}


#[derive(Debug)]
pub struct ModelTestOperation {
    operation: OperationRef,
    engine: EngineRef,
    model_path: ModelPath,
}

impl ModelTestOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, model_path: ModelPath) -> ModelTestOperation {
        ModelTestOperation {
            operation,
            engine,
            model_path,
        }
    }
}

impl Future for ModelTestOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut e = self.engine.write();
        let m = e.model_manager_mut().resolve(&self.model_path)?;
        let res = to_tree(&*m.lock()).unwrap();
        Ok(Async::Ready(Outcome::NodeSet(res.into())))
    }
}

impl OperationImpl for ModelTestOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}


#[derive(Debug)]
pub struct ModelDiffOperation {
    operation: OperationRef,
    engine: EngineRef,
    source: ModelPath,
    target: ModelPath,
    method: DiffMethod,
}

impl ModelDiffOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, source: ModelPath, target: ModelPath, method: DiffMethod) -> ModelDiffOperation {
        ModelDiffOperation {
            operation,
            engine,
            source,
            target,
            method,
        }
    }
}

impl Future for ModelDiffOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut e = self.engine.write();
        let m1 = e.model_manager_mut().resolve(&self.source)?;
        let m2 = e.model_manager_mut().resolve(&self.target)?;
        let diff = match self.method {
            DiffMethod::Minimal => Diff::minimal(m1.lock().root(), m2.lock().root()),
            DiffMethod::Full => Diff::full(m1.lock().root(), m2.lock().root()),
        };
        Ok(Async::Ready(Outcome::NodeSet(to_tree(&diff).unwrap().into())))
    }
}

impl OperationImpl for ModelDiffOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}


#[derive(Debug)]
pub struct ModelUpdateOperation {
    operation: OperationRef,
    engine: EngineRef,
    prev_model: ModelPath,
    next_model: ModelPath,
    dry_run: bool,
    work_op: Option<OperationExec>,
}

impl ModelUpdateOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, prev_model: ModelPath, next_model: ModelPath, dry_run: bool) -> ModelUpdateOperation {
        ModelUpdateOperation {
            operation,
            engine,
            prev_model,
            next_model,
            dry_run,
            work_op: None,
        }
    }
}

impl Future for ModelUpdateOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(ref mut op) = self.work_op {
            if let Async::Ready(Some(p)) = op.progress_mut().poll()? {
                self.operation.write().update_progress_value(p.value());
            }
            if let Async::Ready(outcome) = op.outcome_mut().poll()? {
                Ok(Async::Ready(outcome))
            } else {
                Ok(Async::NotReady)
            }
        } else {
            let mut work_ops = Vec::new();
            {
                let (m1, m2) = {
                    let mut e = self.engine.write();
                    let m1 = e.model_manager_mut().resolve(&self.prev_model)?;
                    let m2 = e.model_manager_mut().resolve(&self.next_model)?;
                    (m1, m2)
                };
                let model1 = m1.lock();
                let model2 = m2.lock();
                let mut update = ModelUpdate::new(&model1, &model2);

                let work_dir = Path::new(".op");
                for p in model2.procs().iter() {
                    if p.kind() == ProcKind::Update {
                        let id = p.id();

                        let changes = update.check_updater(p);
                        if !changes.is_empty() {
                            let mut args = ArgumentsBuilder::new(model2.root());
                            args.set_arg("$changes".into(), &changes.iter().map(|c| to_tree(c).unwrap()).collect::<Vec<_>>().into());
                            args.set_arg("$old".into(), &model1.root().clone().into());

                            let mut w = Work::with_args(Utc::now(), args.build());
                            w.prepare(&model2, p, work_dir)?;
                            w.store()?;

                            let op: OperationRef = Context::ExecWork { bin_id: Uuid::nil(), work_path: w.path().to_path_buf() }.into();
                            work_ops.push(op);

                            println!("Update \"{}\": prepared in {}", id, w.path().display());
                        } else {
                            println!("Update \"{}\": skipped", id);
                        }
                    }
                }
            }

            let op = Context::Sequence(work_ops).into();
            self.work_op = Some(self.engine.enqueue_operation(op, false)?.into_exec());
            self.poll()
        }
    }
}

impl OperationImpl for ModelUpdateOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}


#[derive(Debug)]
pub struct ModelCheckOperation {
    operation: OperationRef,
    engine: EngineRef,
    model_path: ModelPath,
    filter: Option<String>,
    dry_run: bool,
    work_op: Option<OperationExec>,
}

impl ModelCheckOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, model_path: ModelPath, filter: Option<String>, dry_run: bool) -> ModelCheckOperation {
        ModelCheckOperation {
            operation,
            engine,
            model_path,
            dry_run,
            filter,
            work_op: None,
        }
    }
}

impl Future for ModelCheckOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(ref mut op) = self.work_op {
            if let Async::Ready(Some(p)) = op.progress_mut().poll()? {
                self.operation.write().update_progress_value(p.value());
            }
            if let Async::Ready(outcome) = op.outcome_mut().poll()? {
                Ok(Async::Ready(outcome))
            } else {
                Ok(Async::NotReady)
            }
        } else {
            let filter_re = if let Some(ref filter) = self.filter {
                match Regex::new(filter) {
                    Ok(re) => Some(re),
                    Err(err) => {
                        eprintln!("Error while parsing regular expression {:?}: {}", filter, err);
                        None
                    }
                }
            } else {
                None
            };

            let mut work_ops = Vec::new();
            {
                let m = {
                    let mut e = self.engine.write();
                    e.model_manager_mut().resolve(&self.model_path)?
                };
                let model = m.lock();

                let work_dir = Path::new(".op");
                for p in model.procs().iter() {
                    if p.kind() == ProcKind::Check {
                        let id = p.id();
                        if filter_re.is_none() || filter_re.as_ref().unwrap().is_match(id) {
                            let mut w = Work::new(Utc::now());
                            w.prepare(&model, p, work_dir)?;
                            w.store()?;

                            let work_op: OperationRef = Context::ExecWork { bin_id: Uuid::nil(), work_path: w.path().to_path_buf() }.into();
                            work_ops.push(work_op);

                            println!("Check \"{}\": prepared in {}", id, w.path().display());
                        } else {
                            println!("Check \"{}\": skipped", id);
                        }
                    }
                }
            }

            if self.dry_run || work_ops.is_empty() {
                Ok(Async::Ready(Outcome::Empty))
            } else {
                let op = Context::Sequence(work_ops).into();
                self.work_op = Some(self.engine.enqueue_operation(op, false)?.into_exec());
                self.poll()
            }
        }
    }
}

impl OperationImpl for ModelCheckOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}
