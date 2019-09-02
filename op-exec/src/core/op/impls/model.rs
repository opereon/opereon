use kg_tree::diff::ModelDiff;
use regex::Regex;

use super::*;
use crate::core::op::impls::model::ModelOpErrorDetail::QueryOp;
use kg_diag::DiagResultExt;
use kg_tree::opath::Opath;
use op_model::ModelUpdate;
use slog::Logger;
use std::path::{Path, PathBuf};

//pub type ModelOpError = BasicDiag;

#[derive(Debug, Detail, Display)]
pub enum ModelOpErrorDetail {
    #[display(fmt = "cannot query model")]
    QueryOp,
}

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
            _ => Err("unknown diff method".to_string()),
        }
    }
}

#[derive(Debug)]
pub struct ModelInitOperation {
    operation: OperationRef,
    engine: EngineRef,
    path: PathBuf,
}

impl ModelInitOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, path: PathBuf) -> ModelInitOperation {
        ModelInitOperation {
            operation,
            engine,
            path,
        }
    }
}

impl Future for ModelInitOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        ModelManager::init_model(&self.path)?;
        Ok(Async::Ready(Outcome::Empty))
    }
}

impl OperationImpl for ModelInitOperation {
    fn init(&mut self) -> RuntimeResult<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct ModelCommitOperation {
    operation: OperationRef,
    engine: EngineRef,
    message: String,
}

impl ModelCommitOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, message: &str) -> ModelCommitOperation {
        ModelCommitOperation {
            operation,
            engine,
            message: message.to_string(),
        }
    }
}

impl Future for ModelCommitOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut e = self.engine.write();
        let _m = e.model_manager_mut().commit(&self.message)?;
        Ok(Async::Ready(Outcome::Empty))
    }
}

impl OperationImpl for ModelCommitOperation {
    fn init(&mut self) -> RuntimeResult<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct ModelQueryOperation {
    operation: OperationRef,
    engine: EngineRef,
    model_path: ModelPath,
    expr: String,
    logger: Logger,
}

impl ModelQueryOperation {
    pub fn new(
        operation: OperationRef,
        engine: EngineRef,
        model_path: ModelPath,
        expr: String,
    ) -> ModelQueryOperation {
        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "model_path" => model_path.clone(),
            "query" => expr.clone(),
        ));
        ModelQueryOperation {
            operation,
            engine,
            model_path,
            expr,
            logger,
        }
    }
}

impl Future for ModelQueryOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let mut e = self.engine.write();
        let m = e.model_manager_mut().resolve(&self.model_path)?;

        let expr = Opath::parse(&self.expr).map_err_as_cause(|| QueryOp)?;
        info!(self.logger, "Querying model...");
        let res = {
            let m = m.lock();
            kg_tree::set_base_path(m.metadata().path());
            let scope = m.scope()?;
            expr.apply_ext(m.root(), m.root(), &scope)?
        };

        Ok(Async::Ready(Outcome::NodeSet(res.into())))
    }
}

impl OperationImpl for ModelQueryOperation {
    fn init(&mut self) -> RuntimeResult<()> {
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
    pub fn new(
        operation: OperationRef,
        engine: EngineRef,
        model_path: ModelPath,
    ) -> ModelTestOperation {
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
    fn init(&mut self) -> RuntimeResult<()> {
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
    pub fn new(
        operation: OperationRef,
        engine: EngineRef,
        source: ModelPath,
        target: ModelPath,
        method: DiffMethod,
    ) -> ModelDiffOperation {
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
            DiffMethod::Minimal => ModelDiff::minimal(m1.lock().root(), m2.lock().root()),
            DiffMethod::Full => ModelDiff::full(m1.lock().root(), m2.lock().root())?,
        };
        Ok(Async::Ready(Outcome::NodeSet(
            to_tree(&diff).unwrap().into(),
        )))
    }
}

impl OperationImpl for ModelDiffOperation {
    fn init(&mut self) -> RuntimeResult<()> {
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
    proc_op: Option<OperationExec>,
    logger: Logger,
}

impl ModelUpdateOperation {
    pub fn new(
        operation: OperationRef,
        engine: EngineRef,
        prev_model: ModelPath,
        next_model: ModelPath,
        dry_run: bool,
    ) -> ModelUpdateOperation {
        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "old_model" => prev_model.clone(),
            "new_model" => next_model.clone()
        ));
        ModelUpdateOperation {
            operation,
            engine,
            prev_model,
            next_model,
            dry_run,
            proc_op: None,
            logger,
        }
    }
}

impl Future for ModelUpdateOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(ref mut op) = self.proc_op {
            if let Async::Ready(Some(p)) = op.progress_mut().poll()? {
                self.operation.write().update_progress(p);
            }
            if let Async::Ready(outcome) = op.outcome_mut().poll()? {
                Ok(Async::Ready(outcome))
            } else {
                Ok(Async::NotReady)
            }
        } else {
            debug!(self.logger, "Starting model update operation");

            let mut proc_ops = Vec::new();
            {
                let (m1, m2) = {
                    let mut e = self.engine.write();
                    let m1 = e.model_manager_mut().resolve(&self.prev_model)?;
                    let m2 = e.model_manager_mut().resolve(&self.next_model)?;
                    (m1, m2)
                };
                let model1 = m1.lock();
                let model2 = m2.lock();
                let mut update = ModelUpdate::new(&model1, &model2)?;

                let exec_dir = Path::new(".op");

                if model2.procs().is_empty() {
                    info!(self.logger, "Nothing to do, there is no procedures defined in model: [{model_id}]", model_id = model2.metadata().id().to_string(); "verbosity"=>0);
                } else {
                    for p in model2.procs().iter() {
                        if p.kind() == ProcKind::Update {
                            let id = p.id();

                            let (model_changes, file_changes) = update.check_updater(p)?;

                            if model_changes.is_empty() && file_changes.is_empty() {
                                info!(self.logger, "Update [{proc_id}]: skipped - no changes", proc_id = id; "verbosity"=>0);
                                continue;
                            }

                            let mut args = ArgumentsBuilder::new(model2.root());

                            if !model_changes.is_empty() {
                                args.set_arg(
                                    "$model_changes".into(),
                                    &model_changes
                                        .iter()
                                        .map(|c| to_tree(c).unwrap())
                                        .collect::<Vec<_>>()
                                        .into(),
                                );
                            }
                            if !file_changes.is_empty() {
                                args.set_arg(
                                    "$file_changes".into(),
                                    &file_changes
                                        .iter()
                                        .map(|c| to_tree(c).unwrap())
                                        .collect::<Vec<_>>()
                                        .into(),
                                );
                            }
                            args.set_arg("$old".into(), &model1.root().clone().into());

                            let mut e = ProcExec::with_args(Utc::now(), args.build());
                            e.prepare(&model2, p, exec_dir)?;
                            e.store()?;

                            let op: OperationRef = Context::ProcExec {
                                exec_path: e.path().to_path_buf(),
                            }
                                .into();
                            proc_ops.push(op);

                            info!(self.logger, "Update [{proc_id}]: prepared in [{path}]", proc_id = id, path = e.path().display(); "verbosity"=>1);
                        }
                    }
                }
            }

            let op = Context::Sequence(proc_ops).into();
            self.proc_op = Some(self.engine.enqueue_operation(op, false)?.into_exec());
            self.poll()
        }
    }
}

impl OperationImpl for ModelUpdateOperation {
    fn init(&mut self) -> RuntimeResult<()> {
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
    proc_op: Option<OperationExec>,
    logger: Logger,
}

impl ModelCheckOperation {
    pub fn new(
        operation: OperationRef,
        engine: EngineRef,
        model_path: ModelPath,
        filter: Option<String>,
        dry_run: bool,
    ) -> ModelCheckOperation {
        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "model" => model_path.clone(),
            "filter" => filter.clone()
        ));
        ModelCheckOperation {
            operation,
            engine,
            model_path,
            dry_run,
            filter,
            proc_op: None,
            logger,
        }
    }
}

impl Future for ModelCheckOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(ref mut op) = self.proc_op {
            if let Async::Ready(Some(p)) = op.progress_mut().poll()? {
                self.operation.write().update_progress(p);
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
                        error!(self.logger, "Error while parsing regular expression [{filter}]: [{err}]", filter=filter, err=format!("{}", err); "verbosity"=>0);
                        None
                    }
                }
            } else {
                None
            };

            let mut proc_ops = Vec::new();
            {
                let m = {
                    let mut e = self.engine.write();
                    e.model_manager_mut().resolve(&self.model_path)?
                };
                let model = m.lock();

                let exec_dir = Path::new(".op");
                for p in model.procs().iter() {
                    if p.kind() == ProcKind::Check {
                        let id = p.id();
                        if filter_re.is_none() || filter_re.as_ref().unwrap().is_match(id) {
                            let mut e = ProcExec::new(Utc::now());
                            e.prepare(&model, p, exec_dir)?;
                            e.store()?;

                            let proc_op: OperationRef = Context::ProcExec {
                                exec_path: e.path().to_path_buf(),
                            }
                            .into();
                            proc_ops.push(proc_op);

                            info!(self.logger, "Check [{proc_id}]: prepared in {path}", proc_id = id, path=e.path().display(); "verbosity"=>1);
                        } else {
                            info!(self.logger, "Check [{proc_id}]: skipped", proc_id = id; "verbosity"=>1);
                        }
                    }
                }
            }

            if self.dry_run || proc_ops.is_empty() {
                Ok(Async::Ready(Outcome::Empty))
            } else {
                let op = Context::Sequence(proc_ops).into();
                self.proc_op = Some(self.engine.enqueue_operation(op, false)?.into_exec());
                self.poll()
            }
        }
    }
}

impl OperationImpl for ModelCheckOperation {
    fn init(&mut self) -> RuntimeResult<()> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct ModelProbeOperation {
    operation: OperationRef,
    engine: EngineRef,
    ssh_dest: SshDest,
    model_path: ModelPath,
    filter: Option<String>,
    proc_op: Option<OperationExec>,
    logger: Logger,
}

impl ModelProbeOperation {
    pub fn new(
        operation: OperationRef,
        engine: EngineRef,
        ssh_dest: SshDest,
        model_path: ModelPath,
        filter: Option<String>,
        _args: &[(String, String)],
    ) -> ModelProbeOperation {
        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "model" => model_path.clone(),
            "filter" => filter.clone(),
            "ssh_dest" => format!("{:?}", ssh_dest)
        ));
        ModelProbeOperation {
            operation,
            engine,
            ssh_dest,
            model_path,
            filter,
            proc_op: None,
            logger,
        }
    }
}

impl Future for ModelProbeOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Some(ref mut op) = self.proc_op {
            if let Async::Ready(Some(p)) = op.progress_mut().poll()? {
                self.operation.write().update_progress(p);
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
                        error!(self.logger, "Error while parsing regular expression [{filter}]: [{err}]", filter=filter, err=format!("{}", err); "verbosity"=>0);
                        None
                    }
                }
            } else {
                None
            };

            let mut proc_ops = Vec::new();
            {
                let m = {
                    let mut e = self.engine.write();
                    e.model_manager_mut().resolve(&self.model_path)?
                };
                let model = m.lock();

                let exec_dir = Path::new(".op");
                for p in model.procs().iter() {
                    if p.kind() == ProcKind::Probe {
                        let id = p.id();
                        if filter_re.is_none() || filter_re.as_ref().unwrap().is_match(id) {
                            let host = to_tree(&Host::from_dest(self.ssh_dest.clone())).unwrap();
                            let mut args = Arguments::new();
                            args.set_arg(
                                "$host".into(),
                                ArgumentSet::new(&host.into(), model.root()),
                            );

                            let mut e = ProcExec::with_args(Utc::now(), args);
                            e.prepare(&model, p, exec_dir)?;
                            e.store()?;

                            let proc_op: OperationRef = Context::ProcExec {
                                exec_path: e.path().to_path_buf(),
                            }
                            .into();
                            proc_ops.push(proc_op);
                            info!(self.logger, "Probe [{proc_id}]: prepared in [{path}]", proc_id = id, path=e.path().display(); "verbosity"=>1);
                        } else {
                            info!(self.logger, "Probe [{proc_id}]: skipped", proc_id = id; "verbosity"=>1);
                        }
                    }
                }
            }

            if proc_ops.is_empty() {
                Ok(Async::Ready(Outcome::Empty))
            } else {
                let op = Context::Sequence(proc_ops).into();
                self.proc_op = Some(self.engine.enqueue_operation(op, false)?.into_exec());
                self.poll()
            }
        }
    }
}

impl OperationImpl for ModelProbeOperation {
    fn init(&mut self) -> RuntimeResult<()> {
        Ok(())
    }
}
