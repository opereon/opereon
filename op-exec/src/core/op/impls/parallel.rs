use super::*;

#[derive(Debug)]
pub struct ParallelOperation {
    engine: EngineRef,
    operation: OperationRef,
    count: usize,

    steps_sync: Vec<OperationRef>
}

impl ParallelOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, steps: Vec<OperationRef>) -> Result<ParallelOperation, RuntimeError> {
        Ok(ParallelOperation {
            engine,
            operation,
            count: 0,

            steps_sync: steps
        })
    }
}

impl OperationImpl for ParallelOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn execute(&mut self) -> Result<Outcome, RuntimeError> {
        let mut running_ops = vec![];

        for op in self.steps_sync.drain(..) {
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
