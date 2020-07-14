use async_trait::*;
use op_engine::operation::OperationResult;
use op_exec2::command::CommandHandle;

#[async_trait]
pub trait SpawnableCommand {
    async fn spawn(&self) -> OperationResult<CommandHandle>;
}

macro_rules! command_operation_impl {
    ($struct_type:ty) => {
        #[async_trait]
        impl OperationImpl<Outcome> for $struct_type {
            async fn done(
                &mut self,
                _engine: &EngineRef<Outcome>,
                operation: &OperationRef<Outcome>,
            ) -> OperationResult<Outcome> {
                let handle = self.spawn().await?;

                let child = handle.child().clone();
                let mut cancel_rx = operation.write().take_cancel_receiver().unwrap();
                tokio::spawn(async move {
                    if cancel_rx.recv().await.is_some() {
                        child.send_sigterm();
                    }
                });

                let out = handle.wait().await?;

                Ok(Outcome::Command(out))
            }
        }
    };
}

mod combinators;
mod command;
pub mod model;
mod rsync;
