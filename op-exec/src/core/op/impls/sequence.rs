use super::*;

#[derive(Debug)]
pub struct SequenceOperation {
    engine: EngineRef,
    operation: OperationRef,
    current_step: Option<usize>,

    steps_sync: Vec<OperationRef>,
}

impl SequenceOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, steps: Vec<OperationRef>) -> Result<SequenceOperation, RuntimeError> {
        Ok(SequenceOperation {
            engine,
            operation,
            current_step: None,
            steps_sync: steps
        })
    }
}

impl OperationImpl for SequenceOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn execute(&mut self) -> Result<Outcome, RuntimeError> {
        let mut outcomes = vec![];

        for op in self.steps_sync.drain(..) {
            let out = self.engine.execute_operation(op)?;
            outcomes.push(out);
        }

        Ok(Outcome::Many(outcomes))
    }
}

unsafe impl Sync for SequenceOperation {}

unsafe impl Send for SequenceOperation {}
