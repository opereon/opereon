use super::*;


fn cleanup_resources(engine: &EngineRef, resource_id: Uuid) {
    engine.write().resource_manager_mut().remove(resource_id);
}


#[derive(Debug)]
pub struct ExecWorkOperation {
    operation: OperationRef,
    engine: EngineRef,
    op: OperationExec,
}

unsafe impl Sync for ExecWorkOperation {}

unsafe impl Send for ExecWorkOperation {}


impl ExecWorkOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, bin_id: Uuid, work_path: &Path) -> Result<ExecWorkOperation, RuntimeError> {
        let work = engine.write().work_manager_mut().get(work_path)?;
        let steps = {
            let work = work.lock();

            println!("{}: executing work in {}", work.name(), work_path.display());

            let mut steps = Vec::with_capacity(work.jobs().len());
            for i in 0..work.jobs().len() {
                let op: OperationRef = Context::ExecJob {
                    bin_id,
                    work_path: work_path.to_path_buf(),
                    job_index: i,
                }.into();
                steps.push(op);
            }
            steps
        };

        let op: OperationRef = Context::Parallel(steps).into();
        let op = engine.enqueue_operation(op, false)?.into_exec();

        Ok(ExecWorkOperation {
            operation,
            engine,
            op,
        })
    }
}

impl Future for ExecWorkOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Async::Ready(Some(p)) = self.op.progress_mut().poll()? {
            self.operation.write().update_progress_value(p.value());
        }
        if let Async::Ready(outcome) = self.op.outcome_mut().poll()? {
            cleanup_resources(&self.engine, self.operation.read().id());
            Ok(Async::Ready(outcome))
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl OperationImpl for ExecWorkOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}


#[derive(Debug)]
pub struct ExecJobOperation {
    operation: OperationRef,
    engine: EngineRef,
    model: ModelRef,
    op: OperationExec,
}

impl ExecJobOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, bin_id: Uuid, work_path: &Path, job_index: usize) -> Result<ExecJobOperation, RuntimeError> {
        let bin_id = if bin_id.is_nil() {
            operation.read().id()
        } else {
            bin_id
        };

        let work = engine.write().work_manager_mut().get(work_path)?;
        let model = engine.write().model_manager_mut().resolve_bin(work.lock().model_path(), bin_id)?;

        let steps = {
            let w = work.lock();

            let ref job = w.jobs()[job_index];
            println!("{}: executing job in {}", job.host(), job.path().display());

            let mut steps = Vec::with_capacity(job.actions().len());

            for i in 0..job.actions().len() {
                let op: OperationRef = Context::ExecAction {
                    bin_id,
                    work_path: work_path.to_owned(),
                    job_index,
                    action_index: i,
                }.into();
                steps.push(op);
            }

            steps
        };

        let op: OperationRef = Context::Sequence(steps).into();
        let op = engine.enqueue_operation(op, false)?.into_exec();

        Ok(ExecJobOperation {
            operation,
            engine,
            model,
            op,
        })
    }
}

impl Future for ExecJobOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Async::Ready(Some(p)) = self.op.progress_mut().poll()? {
            self.operation.write().update_progress_value(p.value());
        }
        if let Async::Ready(outcome) = self.op.outcome_mut().poll()? {
            cleanup_resources(&self.engine, self.operation.read().id());
            Ok(Async::Ready(outcome))
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl OperationImpl for ExecJobOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

unsafe impl Sync for ExecJobOperation {}

unsafe impl Send for ExecJobOperation {}


#[derive(Debug)]
pub struct ExecActionOperation {
    operation: OperationRef,
    engine: EngineRef,
    bin_id: Uuid,
    work_path: PathBuf,
    job_index: usize,
    action_index: usize,
    work_op: Option<OperationExec>,
}

impl ExecActionOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, bin_id: Uuid, work_path: &Path, job_index: usize, action_index: usize) -> Result<ExecActionOperation, RuntimeError> {
        Ok(ExecActionOperation {
            operation,
            engine,
            bin_id,
            work_path: work_path.to_path_buf(),
            job_index,
            action_index,
            work_op: None,
        })
    }
}

impl Future for ExecActionOperation {
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
            let result = {
                use std::fs::OpenOptions;

                let work = self.engine.write().work_manager_mut().get(&self.work_path)?;
                let model = self.engine.write().model_manager_mut().resolve_bin(work.lock().model_path(), self.bin_id)?;
                let work = work.lock();
                let model = model.lock();
                let ref job = work.jobs()[self.job_index];
                let ref action = job.actions()[self.action_index];
                let proc = model.get_proc_path(work.proc_path()).unwrap();
                let host = model.get_host_path(job.host_path()).unwrap();
                let task = model.get_task_path(action.task_path()).unwrap();

                {
                    let s = proc.scope_mut();
                    s.set_var("$proc".into(), proc.node().clone().into());
                    s.set_var("$host".into(), host.node().clone().into());
                }
                {
                    let s = task.scope_mut();
                    s.set_var("$task".into(), task.node().clone().into());
                }

                let output = OutputLog::new(OpenOptions::new()
                    .write(true)
                    .create(true)
                    .append(true)
                    .open(job.path().join("output.log"))?);

                println!("{}: {}: executing...", job.host(), action);

                let scope = task.scope();
                let base_path = proc.dir();

                let result = match task.kind() {
                    TaskKind::Exec => {
                        let exec = scope.get_var("exec").unwrap();
                        let exec = exec.iter().next().unwrap();
                        let p = model.get_proc(exec).unwrap();

                        let mut args = ArgumentsBuilder::new(model.root());
                        for k in scope.var_names() {
                            if k != "exec" && !k.starts_with('$') {
                                let var = scope.get_var(&k).unwrap();
                                args.set_arg(k.clone(), &var);
                            }
                        }

                        let work_dir = Path::new(".op");
                        let mut w = Work::with_args(Utc::now(), args.build());
                        w.prepare(&model, p, work_dir)?;
                        w.store()?;

                        let op: OperationRef = Context::ExecWork { bin_id: self.bin_id, work_path: w.path().to_path_buf() }.into();
                        self.work_op = Some(self.engine.enqueue_operation(op, false)?.into_exec());
                        return self.poll();
                    }
                    TaskKind::Switch => {
                        let mut p = None;
                        for case in task.switch().unwrap().cases() {
                            let when = case.when().apply_ext(task.root(), task.node(), scope).into_one();
                            if let Some(when) = when {
                                if when.as_boolean() {
                                    p = Some(case.proc());
                                    break;
                                }
                            }
                        }
                        if let Some(p) = p {
                            let mut args = ArgumentsBuilder::new(model.root());
                            for k in proc.scope().var_names() {
                                if !k.starts_with('$') {
                                    let var = scope.get_var(&k).unwrap();
                                    args.set_arg(k, &var);
                                }
                            }
                            for k in scope.var_names() {
                                if !k.starts_with('$') {
                                    let var = scope.get_var(&k).unwrap();
                                    args.set_arg(k, &var);
                                }
                            }

                            let work_dir = Path::new(".op");
                            let mut w = Work::with_args(Utc::now(), args.build());

                            w.prepare(&model, p, work_dir)?;
                            w.store()?;

                            let op: OperationRef = Context::ExecWork { bin_id: self.bin_id, work_path: w.path().to_path_buf() }.into();
                            self.work_op = Some(self.engine.enqueue_operation(op, false)?.into_exec());
                            return self.poll();
                        } else {
                            ActionResult::new(Outcome::Empty, Some(0), None)
                        }
                    }
                    TaskKind::Template => {
                        let src_path: PathBuf = scope.get_var_value("src_path")?;
                        let src_path = base_path.join(src_path);
                        let dst_path: PathBuf = scope.get_var_value_or_default("dst_path", &src_path);
                        let dst_path = job.path().join(dst_path);
                        let mut executor = create_template_executor(job.host(), &self.engine)?;
                        executor.process_template(&self.engine,
                                                  task,
                                                  &src_path,
                                                  &dst_path,
                                                  &output)?
                    }
                    TaskKind::Command => {
                        let cmd: String = scope.get_var_value("cmd")?;
                        let args: Vec<String> = scope.get_var("args").map_or(Vec::new(), |args| args.iter().map(|a| a.as_string()).collect());
                        let out_format = task.output().map(|o| o.format());
                        let mut executor = create_command_executor(job.host(), &self.engine)?;
                        executor.exec_command(&self.engine,
                                              &cmd,
                                              &args,
                                              out_format,
                                              &output)?
                    }
                    TaskKind::Script => {
                        let src_path: PathBuf = scope.get_var_value("src_path")?;
                        let src_path = base_path.join(src_path);
                        let args: Vec<String> = scope.get_var("args").map_or(Vec::new(), |args| args.iter().map(|a| a.as_string()).collect());
                        let cwd: Option<PathBuf> = scope.get_var_value_opt("cwd");
                        let run_as: Option<String> = scope.get_var_value_opt("run_as");
                        let env: Option<EnvVars> = task.env().map(|e| resolve_env(e,
                                                                                            task.root(),
                                                                                            task.node()));
                        let out_format = task.output().map(|o| o.format());
                        let mut executor = create_command_executor(job.host(), &self.engine)?;
                        executor.exec_script(&self.engine,
                                             SourceRef::Path(&src_path),
                                             &args,
                                             env.as_ref(),
                                             cwd.as_ref().map(|p| p.as_ref()),
                                             run_as.as_ref().map(|r| r.as_ref()),
                                             out_format,
                                             &output)?
                    }
                    TaskKind::FileCopy => {
                        let src_path: PathBuf = scope.get_var_value("src_path")?;
                        let src_path = base_path.join(src_path);
                        let dst_path: PathBuf = scope.get_var_value_or_default("dst_path", &src_path);
                        let chown: Option<String> = scope.get_var_value_opt("chown");
                        let chmod: Option<String> = scope.get_var_value_opt("chmod");
                        let mut executor = create_file_executor(job.host(), &self.engine)?;
                        executor.file_compare(&self.engine,
                                              base_path,
                                              &src_path,
                                              &dst_path,
                                              chown.as_ref().map(|s| s.as_ref()),
                                              chmod.as_ref().map(|s| s.as_ref()),
                                              &output)?
                    }
                    TaskKind::FileCompare => {
                        let src_path: PathBuf = scope.get_var_value("src_path")?;
                        let src_path = base_path.join(src_path);
                        let dst_path: PathBuf = scope.get_var_value_or_default("dst_path", &src_path);
                        let chown: Option<String> = scope.get_var_value_opt("chown");
                        let chmod: Option<String> = scope.get_var_value_opt("chmod");
                        let mut executor = create_file_executor(job.host(), &self.engine)?;
                        executor.file_copy(&self.engine,
                                           base_path,
                                           &src_path,
                                           &dst_path,
                                           chown.as_ref().map(|s| s.as_ref()),
                                           chmod.as_ref().map(|s| s.as_ref()),
                                           &output)?
                    }
                };

                print!("{}: {}: result: {}", job.host(), action, result);
                if let Some(out) = task.output() {
                    if let Outcome::NodeSet(ref ns) = *result.outcome() {
                        proc.scope_mut().set_var(out.var().into(), ns.lock().clone());
                        print!(" => ${}", out.var());
                    }
                }
                println!();

                result
            };

            cleanup_resources(&self.engine, self.operation.read().id());
            Ok(Async::Ready(result.into_outcome()))
        }
    }
}

impl OperationImpl for ExecActionOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

unsafe impl Sync for ExecActionOperation {}

unsafe impl Send for ExecActionOperation {}
