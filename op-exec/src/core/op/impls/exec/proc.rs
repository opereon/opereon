use super::*;

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
    ) -> RuntimeResult<ProcExecOperation> {
        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "exec_path" => format!("{}", exec_path.display()),
        ));
        let exec = engine.write().exec_manager_mut().get(exec_path)?;
        let steps = {
            let exec = exec.lock();

            info!(logger, "Executing exec: [{name}] in [{path}]", name=exec.name(), path=exec_path.display(); "verbosity"=>1);

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
            Ok(Async::Ready(outcome))
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl OperationImpl for ProcExecOperation {
    fn init(&mut self) -> RuntimeResult<()> {
        Ok(())
    }
}