use crate::outcome::Outcome;
use crate::state::CoreState;
use async_trait::*;
use kg_tree::serial::to_tree;
use op_engine::operation::OperationResult;
use op_engine::{EngineRef, OperationImpl, OperationRef};
use std::ops::Deref;

pub struct ConfigGetOperation {}

impl ConfigGetOperation {
    pub fn new() -> Self {
        ConfigGetOperation {}
    }
}

#[async_trait]
impl OperationImpl<Outcome> for ConfigGetOperation {
    async fn done(
        &mut self,
        engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<Outcome> {
        let state = engine.state::<CoreState>().unwrap();
        let cfg = to_tree(state.config().deref())?;
        Ok(Outcome::NodeSet(cfg.into()))
    }
}
