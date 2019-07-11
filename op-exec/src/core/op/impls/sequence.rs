use super::*;

#[derive(Debug)]
pub struct SequenceOperation {
    engine: EngineRef,
    operation: OperationRef,

    steps: Vec<OperationRef>,
}

impl SequenceOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, steps: Vec<OperationRef>) -> SequenceOperation {
        SequenceOperation {
            engine,
            operation,
            steps
        }
    }
}

impl OperationImpl for SequenceOperation {
    fn execute(&mut self) -> Result<Outcome, RuntimeError> {
        let mut outcomes = vec![];

        for op in self.steps.drain(..) {
            let out = self.engine.execute_operation(op)?;
            outcomes.push(out);
        }

        Ok(Outcome::Many(outcomes))
    }
}

unsafe impl Sync for SequenceOperation {}

unsafe impl Send for SequenceOperation {}
