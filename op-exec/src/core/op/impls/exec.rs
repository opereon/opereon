use super::*;
use slog::Logger;

fn cleanup_resources(engine: &EngineRef, resource_id: Uuid) {
    engine.write().resource_manager_mut().remove(resource_id);
}

#[derive(Debug)]
pub struct ProcExecOperation {
    operation: OperationRef,
    engine: EngineRef,
    op: OperationExec,
    logger: Logger,
}

unsafe impl Sync for ProcExecOperation {}

unsafe impl Send for ProcExecOperation {}

impl ProcExecOperation {
    pub fn new(
        operation: OperationRef,
        engine: EngineRef,
        exec_path: &Path,
    ) -> Result<ProcExecOperation, RuntimeError> {
        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "exec_path" => format!("{}", exec_path.display()),
        ));
        let exec = engine.write().exec_manager_mut().get(exec_path)?;
        let steps = {
            let exec = exec.lock();

            info!(logger, "Executing exec: [{name}] in [{path}]", name=exec.name(), path=exec_path.display(); "verbosity"=>1);
            //            println!("{}: executing in {}", exec.name(), exec_path.display());

            let mut steps = Vec::with_capacity(exec.run().steps().len());
            for i in 0..exec.run().steps().len() {
                let op: OperationRef = Context::StepExec {
                    exec_path: exec_path.to_path_buf(),
                    step_index: i,
                }
                .into();
                steps.push(op);
            }
            steps
        };

        let op: OperationRef = Context::Sequence(steps).into();
        let op = engine.enqueue_operation(op, false)?.into_exec();

        Ok(ProcExecOperation {
            operation,
            engine,
            op,
            logger,
        })
    }
}

impl Future for ProcExecOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Async::Ready(Some(p)) = self.op.progress_mut().poll()? {
            self.operation.write().update_progress(p);
        }
        if let Async::Ready(outcome) = self.op.outcome_mut().poll()? {
            cleanup_resources(&self.engine, self.operation.read().id());
            Ok(Async::Ready(outcome))
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl OperationImpl for ProcExecOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct StepExecOperation {
    operation: OperationRef,
    engine: EngineRef,
    op: OperationExec,
    logger: Logger,
}

impl StepExecOperation {
    pub fn new(
        operation: OperationRef,
        engine: EngineRef,
        exec_path: &Path,
        step_index: usize,
    ) -> Result<StepExecOperation, RuntimeError> {
        let proc_exec = engine.write().exec_manager_mut().get(exec_path)?;

        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "exec_path" => format!("{}", exec_path.display()),
        ));

        let tasks = {
            let proc_exec = proc_exec.lock();
            let ref step_exec = proc_exec.run().steps()[step_index];

            info!(logger, "Executing step on [{host}] in [{path}]", host=format!("{}",step_exec.host()), path=step_exec.path().display(); "verbosity"=>1);

            let mut tasks = Vec::with_capacity(step_exec.tasks().len());

            for i in 0..step_exec.tasks().len() {
                let op: OperationRef = Context::TaskExec {
                    exec_path: exec_path.to_owned(),
                    step_index,
                    task_index: i,
                }
                .into();
                tasks.push(op);
            }

            tasks
        };

        let op: OperationRef = Context::Sequence(tasks).into();
        let op = engine.enqueue_operation(op, false)?.into_exec();

        Ok(StepExecOperation {
            operation,
            engine,
            op,
            logger,
        })
    }
}

impl Future for StepExecOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if let Async::Ready(Some(p)) = self.op.progress_mut().poll()? {
            self.operation.write().update_progress(p);
        }
        if let Async::Ready(outcome) = self.op.outcome_mut().poll()? {
            cleanup_resources(&self.engine, self.operation.read().id());
            Ok(Async::Ready(outcome))
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl OperationImpl for StepExecOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

#[derive(Debug)]
pub struct TaskExecOperation {
    operation: OperationRef,
    engine: EngineRef,
    exec_path: PathBuf,
    step_index: usize,
    task_index: usize,
    proc_op: Option<OperationExec>,
    logger: Logger,
}

impl TaskExecOperation {
    pub fn new(
        operation: OperationRef,
        engine: EngineRef,
        exec_path: &Path,
        step_index: usize,
        task_index: usize,
    ) -> Result<TaskExecOperation, RuntimeError> {
        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "exec_path" => format!("{}", exec_path.display()),
            "step_index" => step_index,
            "task_index" => task_index,
        ));

        Ok(TaskExecOperation {
            operation,
            engine,
            exec_path: exec_path.to_path_buf(),
            step_index,
            task_index,
            proc_op: None,
            logger,
        })
    }
}

impl Future for TaskExecOperation {
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
            let result = {
                use std::fs::OpenOptions;

                let proc_exec = self
                    .engine
                    .write()
                    .exec_manager_mut()
                    .get(&self.exec_path)?;
                let proc_exec = proc_exec.lock();

                let curr_model = self
                    .engine
                    .write()
                    .model_manager_mut()
                    .resolve(proc_exec.curr_model())?;
                let curr_model = curr_model.lock();

                let ref step_exec = proc_exec.run().steps()[self.step_index];
                let ref task_exec = step_exec.tasks()[self.task_index];

                let proc = curr_model.get_proc_path(proc_exec.proc_path()).unwrap();
                let task = curr_model.get_task_path(task_exec.task_path()).unwrap();

                {
                    let s = proc.scope_mut();
                    s.set_var("$proc".into(), proc.node().clone().into());
                    let host = match step_exec.host_path() {
                        Some(p) => curr_model.get_host_path(p).unwrap().node().clone(),
                        None => to_tree(step_exec.host()).unwrap(),
                    };
                    s.set_var("$host".into(), host.into());
                }
                {
                    let s = task.scope_mut();
                    s.set_var("$task".into(), task.node().clone().into());
                }

                let output = OutputLog::new(
                    OpenOptions::new()
                        .write(true)
                        .create(true)
                        .append(true)
                        .open(step_exec.path().join("output.log"))?,
                );

                info!(self.logger, "Executing task [{exec_name}] on host [{host}] ...", host=format!("{}", step_exec.host()), exec_name=task_exec.name(); "verbosity"=>1);

                let scope = task.scope();
                let base_path = proc.dir();

                let result = match task.kind() {
                    TaskKind::Exec => {
                        let exec = scope.get_var("exec").unwrap();
                        let exec = exec.iter().next().unwrap();
                        let p = curr_model.get_proc(exec).unwrap();

                        let mut args = ArgumentsBuilder::new(curr_model.root());
                        for k in scope.var_names() {
                            if k != "exec" && !k.starts_with('$') {
                                let var = scope.get_var(&k).unwrap();
                                args.set_arg(k.clone(), &var);
                            }
                        }

                        let exec_dir = Path::new(".op");
                        let mut e = ProcExec::with_args(Utc::now(), args.build());
                        e.prepare(&curr_model, p, exec_dir)?;
                        e.store()?;

                        let op: OperationRef = Context::ProcExec {
                            exec_path: e.path().to_path_buf(),
                        }
                        .into();
                        self.proc_op = Some(self.engine.enqueue_operation(op, false)?.into_exec());
                        return self.poll();
                    }
                    TaskKind::Switch => {
                        let mut p = None;
                        for case in task.switch().unwrap().cases() {
                            let when = case
                                .when()
                                .apply_ext(task.root(), task.node(), scope)
                                .into_one();
                            if let Some(when) = when {
                                if when.as_boolean() {
                                    p = Some(case.proc());
                                    break;
                                }
                            }
                        }
                        if let Some(p) = p {
                            let mut args = ArgumentsBuilder::new(curr_model.root());
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

                            let exec_dir = Path::new(".op");
                            let mut e = ProcExec::with_args(Utc::now(), args.build());

                            e.prepare(&curr_model, p, exec_dir)?;
                            e.store()?;

                            let op: OperationRef = Context::ProcExec {
                                exec_path: e.path().to_path_buf(),
                            }
                            .into();
                            self.proc_op =
                                Some(self.engine.enqueue_operation(op, false)?.into_exec());
                            return self.poll();
                        } else {
                            TaskResult::new(Outcome::Empty, Some(0), None)
                        }
                    }
                    TaskKind::Template => {
                        let src_path: PathBuf = scope.get_var_value("src_path")?;
                        let src_path = base_path.join(src_path);
                        let dst_path: PathBuf =
                            scope.get_var_value_or_default("dst_path", &src_path);
                        let dst_path = step_exec.path().join(dst_path);
                        let mut executor =
                            create_template_executor(step_exec.host(), &self.engine)?;
                        executor.process_template(
                            &self.engine,
                            task,
                            &src_path,
                            &dst_path,
                            &output,
                        )?
                    }
                    TaskKind::Command => {
                        let cmd: String = scope.get_var_value("cmd")?;
                        let args: Vec<String> = scope.get_var("args").map_or(Vec::new(), |args| {
                            args.iter().map(|a| a.as_string()).collect()
                        });
                        let out_format = task.output().map(|o| o.format());
                        let mut executor = create_command_executor(step_exec.host(), &self.engine)?;
                        executor.exec_command(&self.engine, &cmd, &args, out_format, &output)?
                    }
                    TaskKind::Script => {
                        let src_path: PathBuf = scope.get_var_value("src_path")?;
                        let src_path = base_path.join(src_path);
                        let args: Vec<String> = scope.get_var("args").map_or(Vec::new(), |args| {
                            args.iter().map(|a| a.as_string()).collect()
                        });
                        let cwd: Option<PathBuf> = scope.get_var_value_opt("cwd");
                        let run_as: Option<String> = scope.get_var_value_opt("run_as");
                        let env: Option<EnvVars> = task
                            .env()
                            .map(|e| resolve_env(e, task.root(), task.node(), task.scope()));
                        let out_format = task.output().map(|o| o.format());
                        let mut executor = create_command_executor(step_exec.host(), &self.engine)?;
                        executor.exec_script(
                            &self.engine,
                            SourceRef::Path(&src_path),
                            &args,
                            env.as_ref(),
                            cwd.as_ref().map(|p| p.as_ref()),
                            run_as.as_ref().map(|r| r.as_ref()),
                            out_format,
                            &output,
                        )?
                    }
                    TaskKind::FileCopy => {
                        let src_path: PathBuf = scope.get_var_value("src_path")?;
                        let src_path = base_path.join(src_path);
                        let dst_path: PathBuf =
                            scope.get_var_value_or_default("dst_path", &src_path);
                        let chown: Option<String> = scope.get_var_value_opt("chown");
                        let chmod: Option<String> = scope.get_var_value_opt("chmod");
                        let op: OperationRef = Context::FileCopyExec {
                            curr_dir: base_path.to_path_buf(),
                            src_path,
                            dst_path,
                            chown,
                            chmod,
                            host: step_exec.host().clone(),
                        }
                        .into();
                        self.proc_op = Some(self.engine.enqueue_operation(op, false)?.into_exec());
                        return self.poll();
                    }
                    TaskKind::FileCompare => {
                        let src_path: PathBuf = scope.get_var_value("src_path")?;
                        let src_path = base_path.join(src_path);
                        let dst_path: PathBuf =
                            scope.get_var_value_or_default("dst_path", &src_path);
                        let chown: Option<String> = scope.get_var_value_opt("chown");
                        let chmod: Option<String> = scope.get_var_value_opt("chmod");
                        let mut executor = create_file_executor(step_exec.host(), &self.engine)?;
                        executor
                            .file_compare(
                                &self.engine,
                                base_path,
                                &src_path,
                                &dst_path,
                                chown.as_ref().map(|s| s.as_ref()),
                                chmod.as_ref().map(|s| s.as_ref()),
                                true,
                            )?
                            .into_task_result()
                    }
                };
                info!(self.logger, "Task [{task_name}] finished on [{host}]. Result : [{result}]",
                      task_name = task_exec.name(),
                      host = format!("{}", step_exec.host()),
                      result = format!("{}", result);
                      "verbosity" => 0
                );
                //                print!("{}: {}: result: {}", step_exec.host(), task_exec, result);

                if let Some(out) = task.output() {
                    if let Outcome::NodeSet(ref ns) = *result.outcome() {
                        out.apply(
                            task.root(),
                            task.node(),
                            proc.scope_mut(),
                            ns.lock().clone(),
                        );
                        //print!(" => ${}", out.var());
                    }
                }
                result
            };

            cleanup_resources(&self.engine, self.operation.read().id());
            Ok(Async::Ready(result.into_outcome()))
        }
    }
}

impl OperationImpl for TaskExecOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}
