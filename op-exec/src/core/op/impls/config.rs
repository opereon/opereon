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

impl Future for ConfigGetOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let config = to_tree(self.engine.read().config().deref()).unwrap();
        Ok(Async::Ready(Outcome::NodeSet(config.into())))
    }
}

impl OperationImpl for ConfigGetOperation { }

