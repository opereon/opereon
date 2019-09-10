use crate::{OperationRef, EngineRef, OperationExec, Outcome, RuntimeError, OperationImpl};
use slog::Logger;
use std::path::Path;
use tokio::prelude::{Async, Future, Poll, Stream};
use crate::core::error::RuntimeResult;
use crate::Context;

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
    ) -> RuntimeResult<StepExecOperation> {
        let proc_exec = engine.write().exec_manager_mut().get(exec_path)?;

        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "exec_path" => format!("{}", exec_path.display()),
        ));

        let tasks = {
            let proc_exec = proc_exec.lock();
            let step_exec = &proc_exec.run().steps()[step_index];

//            info!(logger, "Executing step on [{host}] in [{path}]", host= step_exec.host().to_string(), path=step_exec.path().display(); "verbosity"=>"1");

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
            Ok(Async::Ready(outcome))
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl OperationImpl for StepExecOperation {
    fn init(&mut self) -> RuntimeResult<()> {
        Ok(())
    }
}
