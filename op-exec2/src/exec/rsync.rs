use async_trait::async_trait;

use op_async::{OperationImpl, EngineRef, OperationRef, ProgressUpdate, OperationError};
use kg_diag::BasicDiag;
use op_async::operation::OperationResult;


struct CompareOperation {

}

impl CompareOperation {
    pub fn new() -> Self {
        Self {}
    }
}
type Outcome = ();
#[async_trait]
impl OperationImpl<Outcome> for CompareOperation {
    async fn init(&mut self, _engine: &EngineRef<()>, _operation: &OperationRef<()>) -> OperationResult<()> {
        Ok(())
    }

    async fn next_progress(&mut self, engine: &EngineRef<Outcome>, operation: &OperationRef<Outcome>) -> OperationResult<ProgressUpdate> {
        Ok(ProgressUpdate::done())
    }

    async fn done(&mut self, _engine: &EngineRef<()>, _operation: &OperationRef<()>) -> OperationResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_operation_test() {
        let engine: EngineRef<()> = EngineRef::new();

        let op_impl = CompareOperation::new();

        let op = OperationRef::new("compare_operation", op_impl);

        engine.enqueue_operation(op);

        engine.run();

    }
}