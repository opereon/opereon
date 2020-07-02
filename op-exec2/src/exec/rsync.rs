use async_trait::async_trait;

use crate::outcome::Outcome;

use crate::rsync::compare::State;
use crate::rsync::copy::ProgressInfo;
use crate::rsync::{DiffInfo, RsyncCompare, RsyncConfig, RsyncCopy, RsyncParams, RsyncResult};
use crate::OutputLog;

use op_async::operation::OperationResult;
use op_async::progress::{Progress, Unit};
use op_async::{EngineRef, OperationImpl, OperationRef, ProgressUpdate};

use crate::utils::SharedChildExt;
use shared_child::SharedChild;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

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

        let cancel_rx = operation.write().take_cancel_receiver().unwrap();
        handle_cancel(cancel_rx, cmp.child().clone());

        let res = cmp.output().await?;

        Ok(Outcome::FileDiff(res))
    }
}

fn handle_cancel(mut cancel_rx: mpsc::Receiver<()>, child: Arc<SharedChild>) {
    tokio::spawn(async move {
        if cancel_rx.recv().await.is_some() {
            child.send_sigterm();
        }
    });
}

struct FileCopyOperation {
    config: RsyncConfig,
    params: RsyncParams,
    checksum: bool,
    log: OutputLog,
    progress_receiver: Option<mpsc::UnboundedReceiver<ProgressInfo>>,
    done_receiver: Option<oneshot::Receiver<RsyncResult<()>>>,
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
        let op_impl =
            FileCompareOperation::new(&self.config, &self.params, self.checksum, &self.log);
        let op = OperationRef::new("compare_operation", op_impl);

        let out = engine.enqueue_with_res(op).await?;
        let diffs = if let Outcome::FileDiff(res) = out {
            res
        } else {
            unreachable!()
        };

        *operation.write().progress_mut() = build_progress(&diffs);

        let (progress_tx, progress_rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = oneshot::channel();
        self.progress_receiver = Some(progress_rx);
        self.done_receiver = Some(done_rx);

        let config = self.config.clone();
        let params = self.params.clone();
        let log = self.log.clone();

        let cancel_rx = operation.write().take_cancel_receiver().unwrap();

        tokio::spawn(async move {
            match RsyncCopy::spawn(&config, &params, progress_tx, &log) {
                Ok(copy) => {
                    handle_cancel(cancel_rx, copy.child().clone());
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
            let update = if progress.is_completed {
                ProgressUpdate::partial_done(progress.file_name)
            } else {
                ProgressUpdate::new_partial(progress.loaded_bytes, progress.file_name)
            };
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
        rx.await.expect("Sender dropped before completion")?;
        Ok(Outcome::Empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use tokio::time::Duration;

    #[test]
    fn cancel_file_copy_operation_test() {
        let engine: EngineRef<Outcome> = EngineRef::new();

        let mut rt = EngineRef::<()>::build_runtime();

        let cfg = RsyncConfig::default();
        let params = RsyncParams::new("./", "./../target/debug/incremental", "./../target/debug2");
        let log = OutputLog::new();

        let op_impl = FileCopyOperation::new(&cfg, &params, false, &log);
        let op = OperationRef::new("copy_operation", op_impl);

        rt.block_on(async move {
            let e = engine.clone();
            tokio::spawn(async move {
                let o = op.clone();

                tokio::spawn(async move {
                    tokio::time::delay_for(Duration::from_secs(3)).await;
                    println!("Stopping operation");
                    o.cancel().await
                });

                engine.register_progress_cb(|_e, o| eprintln!("progress: {}", o.read().progress()));

                let res = engine.enqueue_with_res(op).await.unwrap_err();
                println!("operation completed with error {}", res);
                engine.stop();
            });

            e.start().await;
            // eprintln!("log = {}", log);
            println!("Engine stopped");
        })
    }

    #[test]
    fn file_copy_operation_test() {
        let engine: EngineRef<Outcome> = EngineRef::new();

        let mut rt = EngineRef::<()>::build_runtime();
        let cfg = RsyncConfig::default();
        let params = RsyncParams::new("./", "./../target/debug/incremental", "./../target/debug2");
        let log = OutputLog::new();

        let op_impl = FileCopyOperation::new(&cfg, &params, false, &log);
        let op = OperationRef::new("copy_operation", op_impl);

        rt.block_on(async move {
            let e = engine.clone();
            tokio::spawn(async move {
                engine.register_progress_cb(|_e, o| eprintln!("progress: {}", o.read().progress()));

                let res = engine.enqueue_with_res(op).await.unwrap();
                println!("operation completed {:?}", res);
                engine.stop();
            });

            e.start().await;
            // eprintln!("log = {}", log);
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
                let params =
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
                let params =
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
