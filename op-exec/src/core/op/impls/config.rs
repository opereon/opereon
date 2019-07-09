use super::*;

#[derive(Debug)]
pub struct ConfigGetOperation {
    operation: OperationRef,
    engine: EngineRef,
}

impl ConfigGetOperation {
    pub fn new(operation: OperationRef, engine: EngineRef) -> ConfigGetOperation {
        ConfigGetOperation {
            operation,
            engine,
        }
    }
}

impl OperationImpl for ConfigGetOperation {
    fn execute(&mut self) -> Result<Outcome, RuntimeError>{
        let config = to_tree(self.engine.read().config().deref()).unwrap();
        Ok(Outcome::NodeSet(config.into()))
    }
}

