use async_trait::async_trait;

use crate::outcome::Outcome;

use crate::rsync::compare::State;
use crate::rsync::copy::ProgressInfo;
use crate::rsync::{DiffInfo, RsyncCompare, RsyncConfig, RsyncCopy, RsyncParams, RsyncResult, FileCopyErrorDetail};
use crate::OutputLog;

use op_async::operation::OperationResult;
use op_async::progress::{Progress, Unit};
use op_async::{EngineRef, OperationImpl, OperationRef, ProgressUpdate};

use kg_diag::{BasicDiag, IntoDiagRes, Severity};
use std::process::ExitStatus;
use tokio::sync::{mpsc, oneshot};
use crate::utils::SharedChildExt;

struct FileCompareOperation {
    config: RsyncConfig,
    params: RsyncParams,
    checksum: bool,
    log: OutputLog,
}

impl FileCompareOperation {
    pub fn new(
        config: &RsyncConfig,
        params: &RsyncParams,
        checksum: bool,
        log: &OutputLog,
    ) -> Self {
        Self {
            config: config.clone(),
            params: params.clone(),
            checksum,
            log: log.clone(),
        }
    }
}

#[async_trait]
impl OperationImpl<Outcome> for FileCompareOperation {
    async fn done(
        &mut self,
        _engine: &EngineRef<Outcome>,
        operation: &OperationRef<Outcome>,
    ) -> OperationResult<Outcome> {
        let cmp = RsyncCompare::spawn(&self.config, &self.params, self.checksum, &self.log)?;

        let child = cmp.child().clone();
        let mut cancel_rx = operation.write().take_cancel_receiver().unwrap();

        tokio::spawn(async move {
            if let Some(_) = cancel_rx.recv().await {
                child.send_sigterm();
            }
        });

        let res = cmp.output().await?;

        Ok(Outcome::FileDiff(res))
    }
}

struct FileCopyOperation {
    config: RsyncConfig,
    params: RsyncParams,
    checksum: bool,
    log: OutputLog,
    progress_receiver: Option<mpsc::UnboundedReceiver<ProgressInfo>>,
    done_receiver: Option<oneshot::Receiver<RsyncResult<ExitStatus>>>,
}

impl FileCopyOperation {
    pub fn new(
        config: &RsyncConfig,
        params: &RsyncParams,
        checksum: bool,
        log: &OutputLog,
    ) -> Self {
        Self {
            config: config.clone(),
            params: params.clone(),
            checksum,
            log: log.clone(),
            progress_receiver: None,
            done_receiver: None,
        }
    }
}

fn build_progress(diffs: &Vec<DiffInfo>) -> Progress {
    let mut parts = vec![];

    for diff in diffs {
        if let State::Missing | State::Modified(_) = diff.state() {
            parts.push(Progress::new_partial(
                &diff.file_path().to_string_lossy(),
                0.,
                diff.file_size() as f64,
                Unit::Bytes,
            ));
        }
    }
    let progress = Progress::from_parts(parts);
    progress
}

#[async_trait]
impl OperationImpl<Outcome> for FileCopyOperation {
    async fn init(
        &mut self,
        engine: &EngineRef<Outcome>,
        operation: &OperationRef<Outcome>,
    ) -> OperationResult<()> {
        let op_impl = FileCompareOperation::new(&self.config, &self.params, self.checksum, &self.log);
        let op = OperationRef::new("compare_operation", op_impl);

        let out = engine.enqueue_with_res(op).await?;
        let diffs = if let Outcome::FileDiff(res) = out {
            match res.status() {
                Some(code) => {
                    if code == 0 {
                        res
                    } else {
                        return Err(FileCopyErrorDetail::CompareFailed { code }).into_diag_res();
                    }
                }
                None => {
                    return Err(FileCopyErrorDetail::CompareCanceled).into_diag_res();
                }
            }
        } else {
            unreachable!()
        };

        *operation.write().progress_mut() = build_progress(diffs.diffs());

        let (progress_tx, progress_rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = oneshot::channel();
        self.progress_receiver = Some(progress_rx);
        self.done_receiver = Some(done_rx);

        let config = self.config.clone();
        let params = self.params.clone();
        let log = self.log.clone();

        tokio::spawn(async move {
            match RsyncCopy::spawn(&config, &params, progress_tx, &log) {
                Ok(copy) => {
                    let res = copy.wait().await;
                    let _ = done_tx.send(res);
                }
                Err(err) => {
                    let _ = done_tx.send(Err(err));
                }
            };
        });

        Ok(())
    }

    async fn next_progress(
        &mut self,
        _engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<ProgressUpdate> {
        let res = self
            .progress_receiver
            .as_mut()
            .expect("progress_receiver not set!")
            .recv()
            .await;
        if let Some(progress) = res {
            let update = ProgressUpdate::new_partial(progress.loaded_bytes, progress.file_name);
            Ok(update)
        } else {
            Ok(ProgressUpdate::done())
        }
    }

    async fn done(
        &mut self,
        _engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<Outcome> {
        let rx = self.done_receiver.take().expect("done_receiver not set!");
        let result = rx.await.expect("Sender dropped before completion")?;
        Ok(Outcome::FileCopy {
            status: result.code()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rsync::DiffInfo;
    use tokio::time::Duration;

    #[test]
    fn cancel_file_copy_operation_test() {
        let engine: EngineRef<Outcome> = EngineRef::new();

        let mut rt = EngineRef::<()>::build_runtime();

        let cfg = RsyncConfig::default();
        let mut params =
            RsyncParams::new("./", "./../target/debug/incremental", "./../target/debug2");
        let log = OutputLog::new();

        let op_impl = FileCopyOperation::new(&cfg, &params, false, &log);
        let op = OperationRef::new("copy_operation", op_impl);

        rt.block_on(async move {
            let e = engine.clone();
            tokio::spawn(async move {
                let o = op.clone();

                tokio::spawn(async move {
                    tokio::time::delay_for(Duration::from_secs(2)).await;
                    println!("Stopping operation");
                    o.cancel().await
                });

                engine.register_progress_cb(|e, o| eprintln!("progress: {}", o.read().progress()));

                let res = engine.enqueue_with_res(op).await.unwrap();
                println!("operation completed {:?}", res);
                engine.stop();
            });

            e.start().await;
            println!("Engine stopped");
        })
    }

    #[test]
    fn file_copy_operation_test() {
        let engine: EngineRef<Outcome> = EngineRef::new();

        let mut rt = EngineRef::<()>::build_runtime();
        let cfg = RsyncConfig::default();
        let mut params =
            RsyncParams::new("./", "./../target/debug/incremental", "./../target/debug2");
        let log = OutputLog::new();

        let op_impl = FileCopyOperation::new(&cfg, &params, false, &log);
        let op = OperationRef::new("copy_operation", op_impl);

        rt.block_on(async move {
            let e = engine.clone();
            tokio::spawn(async move {
                engine.register_progress_cb(|e, o| eprintln!("progress: {}", o.read().progress()));

                let res = engine.enqueue_with_res(op).await.unwrap();
                println!("operation completed {:?}", res);
                engine.stop();
            });

            e.start().await;
            println!("Engine stopped");
        })
    }

    #[test]
    fn compare_operation_test() {
        let engine: EngineRef<Outcome> = EngineRef::new();

        let mut rt = EngineRef::<()>::build_runtime();

        rt.block_on(async move {
            let e = engine.clone();
            tokio::spawn(async move {
                let cfg = RsyncConfig::default();
                let mut params =
                    RsyncParams::new("./", "./../target/debug/incremental", "./../target/debug2");
                let log = OutputLog::new();

                let op_impl = FileCompareOperation::new(&cfg, &params, false, &log);
                let op = OperationRef::new("compare_operation", op_impl);
                let res = engine.enqueue_with_res(op).await.unwrap();
                println!("operation completed {:?}", res);
                engine.stop();
            });

            e.start().await;
            println!("Engine stopped");
        })
    }

    #[test]
    fn cancel_compare_operation_test() {
        let engine: EngineRef<Outcome> = EngineRef::new();

        let mut rt = EngineRef::<()>::build_runtime();

        rt.block_on(async move {
            let e = engine.clone();
            tokio::spawn(async move {
                let cfg = RsyncConfig::default();
                let mut params =
                    RsyncParams::new("./", "./../target/debug/incremental", "./../target/debug2");
                let log = OutputLog::new();

                let op_impl = FileCompareOperation::new(&cfg, &params, true, &log);
                let op = OperationRef::new("compare_operation", op_impl);

                let o = op.clone();
                tokio::spawn(async move {
                    tokio::time::delay_for(Duration::from_secs(2)).await;
                    println!("Stopping compare operation...");
                    o.cancel().await
                });

                let err = engine.enqueue_with_res(op).await.unwrap_err();
                println!("operation completed with error {}", err);
                engine.stop();
            });

            e.start().await;
            println!("Engine stopped");
        })
    }

}
