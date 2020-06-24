use async_trait::async_trait;

use op_async::{OperationImpl, EngineRef, OperationRef, ProgressUpdate, OperationError};
use kg_diag::BasicDiag;
use op_async::operation::OperationResult;
use crate::rsync::{RsyncConfig, RsyncParams, DiffInfo, rsync_compare};
use crate::OutputLog;

struct CompareOperation {
    config: RsyncConfig,
    params: RsyncParams,
    checksum: bool,
    log: OutputLog,
}

impl CompareOperation {
    pub fn new(    config: &RsyncConfig,
                   params: &RsyncParams,
                   checksum: bool,
                   log: &OutputLog,) -> Self {
        Self {
            config: config.clone(),
            params: params.clone(),
            checksum,
            log: log.clone()
        }
    }
}
type Outcome = Vec<DiffInfo>;
#[async_trait]
impl OperationImpl<Outcome> for CompareOperation {
    async fn init(&mut self, _engine: &EngineRef<Outcome>, _operation: &OperationRef<Outcome>) -> OperationResult<()> {
        Ok(())
    }

    async fn next_progress(&mut self, engine: &EngineRef<Outcome>, operation: &OperationRef<Outcome>) -> OperationResult<ProgressUpdate> {
        Ok(ProgressUpdate::done())
    }

    async fn done(&mut self, _engine: &EngineRef<Outcome>, _operation: &OperationRef<Outcome>) -> OperationResult<Outcome> {
        let res = rsync_compare(&self.config, &self.params, self.checksum, &self.log).await?;
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rsync::DiffInfo;

    #[test]
    fn compare_operation_test() {
        let engine: EngineRef<Vec<DiffInfo>> = EngineRef::new();

        let mut rt = EngineRef::<()>::build_runtime();

        rt.block_on(async move  {
            let e = engine.clone();
            tokio::spawn(async move {
                let cfg = RsyncConfig::default();
                let mut params =
                    RsyncParams::new("./", "./../target/debug/incremental", "./../target/debug2");
                let log = OutputLog::new();

                let op_impl = CompareOperation::new(&cfg, &params, false, &log);
                let op = OperationRef::new("compare_operation", op_impl);
                let res = engine.enqueue_with_res(op).await;
                println!("operation completed {:#?}", res);
                engine.stop();
            });

            e.start().await;
            println!("Engine stopped");

        })


    }
}