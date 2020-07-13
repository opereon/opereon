use op_engine::{OperationImpl, OperationRef, EngineRef};
use crate::outcome::Outcome;
use op_engine::operation::OperationResult;
use async_trait::*;
pub struct ModelQueryOperation {

}

impl ModelQueryOperation {
    pub fn new() -> Self {
        ModelQueryOperation {

        }
    }
}

#[async_trait]
impl OperationImpl<Outcome> for ModelQueryOperation {
    async fn done(&mut self, _engine: &EngineRef<Outcome>, _operation: &OperationRef<Outcome>) -> OperationResult<Outcome> {
        // TODO ws
        Ok(Outcome::Empty)
    }
}