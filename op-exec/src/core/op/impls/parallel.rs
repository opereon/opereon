use super::*;

#[derive(Debug)]
pub struct ParallelOperation {
    engine: EngineRef,
    operation: OperationRef,
    steps: Vec<OperationRef>
}

impl ParallelOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, steps: Vec<OperationRef>) -> ParallelOperation {
        ParallelOperation {
            engine,
            operation,
            steps
        }
    }
}

impl OperationImpl for ParallelOperation {
    fn execute(&mut self) -> Result<Outcome, RuntimeError> {
        let mut running_ops = vec![];

        for op in self.steps.drain(..) {
            let out = self.engine.start_operation(op);
            running_ops.push(out);
        }

        let mut outcomes = Vec::with_capacity(running_ops.len());
        for rec in running_ops {
            // fail on first error
            outcomes.push(rec.receive()?)
        }
        Ok(Outcome::Many(outcomes))
    }
}

unsafe impl Sync for ParallelOperation {}

unsafe impl Send for ParallelOperation {}
