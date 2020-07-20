use crate::ops::combinators::handle_cancel;
use crate::outcome::Outcome;
use async_trait::*;

use op_engine::operation::OperationResult;
use op_engine::{EngineRef, OperationImpl, OperationRef, ProgressUpdate};
use tokio::task::JoinHandle;

#[derive(Copy, Clone, Debug)]
pub enum ParallelPolicy {
    /// Get outcome of each operation or error if any of them fail
    All,

    /// Get result of operation that completes first
    First,
    // TODO more variants of parallel execution
}

impl Default for ParallelPolicy {
    fn default() -> Self {
        ParallelPolicy::All
    }
}

pub struct ParallelOperation {
    ops: Vec<OperationRef<Outcome>>,
    policy: ParallelPolicy,
    done_handle: Option<JoinHandle<OperationResult<Vec<Outcome>>>>,
}

impl ParallelOperation {
    pub fn new(ops: Vec<OperationRef<Outcome>>) -> Self {
        ParallelOperation::with_policy(ops, ParallelPolicy::default())
    }

    pub fn with_policy(ops: Vec<OperationRef<Outcome>>, policy: ParallelPolicy) -> Self {
        ParallelOperation {
            ops,
            policy,
            done_handle: None,
        }
    }
}

#[async_trait]
impl OperationImpl<Outcome> for ParallelOperation {
    async fn init(
        &mut self,
        engine: &EngineRef<Outcome>,
        operation: &OperationRef<Outcome>,
    ) -> OperationResult<()> {
        handle_cancel(self.ops.clone(), operation);

        let mut futs = vec![];
        use futures::FutureExt;
        for op in self.ops.iter() {
            futs.push(engine.enqueue_with_res(op.clone()).boxed())
        }

        let done_handle = match self.policy {
            ParallelPolicy::All => tokio::spawn(async {
                let results = futures::future::try_join_all(futs).await;
                results
            }),
            ParallelPolicy::First => tokio::spawn(async {
                let fut = futures::future::select_all(futs);
                let (res, _idx, _rest) = fut.await;
                res.map(|o| vec![o])
            }),
        };
        self.done_handle = Some(done_handle);
        Ok(())
    }

    async fn next_progress(
        &mut self,
        _engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<ProgressUpdate> {
        Ok(ProgressUpdate::done())
    }

    async fn done(
        &mut self,
        _engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<Outcome> {
        let done_handle = self.done_handle.take().unwrap();

        let out = done_handle.await.expect("Parallel task panicked")?;
        Ok(Outcome::Many(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::Outcome;
    use kg_diag::IntoDiagRes;
    use kg_diag::Severity;
    use op_engine::operation::{OperationImplExt, OperationResult};
    use op_engine::{EngineRef, OperationImpl, OperationRef};
    use tokio::time::Duration;

    use crate::ops::combinators::parallel::ParallelOperation;

    pub struct TestOp {
        should_fail: bool,
        duration: u64,
    }

    #[derive(Debug, Detail, Display)]
    pub enum TestErr {
        #[display(fmt = "empty error message")]
        Dummy(String),
    }

    impl TestOp {
        pub fn new_op(duration: u64) -> OperationRef<Outcome> {
            let op_impl = TestOp {
                should_fail: false,
                duration,
            };
            OperationRef::new("test_op", op_impl.boxed())
        }
        pub fn new_op_fail(duration: u64) -> OperationRef<Outcome> {
            let op_impl = TestOp {
                should_fail: true,
                duration,
            };
            OperationRef::new("test_op", op_impl.boxed())
        }
    }

    #[async_trait]
    impl OperationImpl<Outcome> for TestOp {
        async fn done(
            &mut self,
            _engine: &EngineRef<Outcome>,
            operation: &OperationRef<Outcome>,
        ) -> OperationResult<Outcome> {
            println!("test operation started..");

            let mut cancel_rx = operation.write().take_cancel_receiver().unwrap();
            let cancel_fut = Box::pin(async move {
                cancel_rx.recv().await;
                println!("Cancel signal received, stopping");
            });

            let delay_fut = tokio::time::delay_for(Duration::from_secs(self.duration));

            let _res = futures::future::select(cancel_fut, delay_fut).await;

            if self.should_fail {
                println!("test operation error!");
                Err(TestErr::Dummy(String::from("Test err"))).into_diag_res()
            } else {
                println!("test operation finished!");
                Ok(Outcome::Empty)
            }
        }
    }

    #[test]
    fn parallel_operation_test() {
        let engine: EngineRef<Outcome> = EngineRef::default();
        let mut rt = EngineRef::<()>::build_runtime();

        let ops = vec![TestOp::new_op(1), TestOp::new_op(1), TestOp::new_op(1)];

        let op_impl = ParallelOperation::new(ops);
        let op = OperationRef::new("parallel_operation", op_impl.boxed());

        rt.block_on(async move {
            let e = engine.clone();
            tokio::spawn(async move {
                let res = engine.enqueue_with_res(op).await.unwrap();
                if let Outcome::Many(outs) = res {
                    println!("finished: {:?}", outs);
                    assert_eq!(outs.len(), 3);
                    engine.stop();
                } else {
                    panic!();
                }
            });

            e.start().await;
            // eprintln!("log = {}", log);
            println!("Engine stopped");
        })
    }

    #[test]
    fn parallel_operation_first_test() {
        let engine: EngineRef<Outcome> = EngineRef::default();
        let mut rt = EngineRef::<()>::build_runtime();

        let ops = vec![TestOp::new_op(1), TestOp::new_op(1), TestOp::new_op(1)];

        let op_impl = ParallelOperation::with_policy(ops, ParallelPolicy::First);
        let op = OperationRef::new("parallel_operation", op_impl.boxed());

        rt.block_on(async move {
            let e = engine.clone();
            tokio::spawn(async move {
                let res = engine.enqueue_with_res(op).await.unwrap();
                if let Outcome::Many(outs) = res {
                    println!("finished: {:?}", outs);
                    assert_eq!(outs.len(), 1);
                    engine.stop();
                } else {
                    panic!();
                }
            });

            e.start().await;
            // eprintln!("log = {}", log);
            println!("Engine stopped");
        })
    }

    #[test]
    fn cancel_parallel_operation_test() {
        let engine: EngineRef<Outcome> = EngineRef::default();
        let mut rt = EngineRef::<()>::build_runtime();

        let ops = vec![TestOp::new_op(5), TestOp::new_op(5), TestOp::new_op(5)];

        let op_impl = ParallelOperation::new(ops);
        let op = OperationRef::new("parallel_operation", op_impl.boxed());

        rt.block_on(async move {
            let e = engine.clone();
            tokio::spawn(async move {
                let o = op.clone();

                tokio::spawn(async move {
                    tokio::time::delay_for(Duration::from_secs(2)).await;
                    println!("Stopping operation");
                    o.cancel().await
                });

                let res = engine.enqueue_with_res(op).await.unwrap();
                println!("operation completed with {:?}", res);
                engine.stop();
            });

            e.start().await;
            // eprintln!("log = {}", log);
            println!("Engine stopped");
        })
    }

    #[test]
    fn parallel_operation_err_test() {
        let engine: EngineRef<Outcome> = EngineRef::default();
        let mut rt = EngineRef::<()>::build_runtime();

        let ops = vec![
            TestOp::new_op(5),
            TestOp::new_op(5),
            TestOp::new_op(5),
            TestOp::new_op_fail(1),
        ];

        let op_impl = ParallelOperation::new(ops);
        let op = OperationRef::new("parallel_operation", op_impl.boxed());

        rt.block_on(async move {
            let e = engine.clone();
            tokio::spawn(async move {
                let res = engine.enqueue_with_res(op).await.unwrap_err();
                println!("operation completed with {}", res);
                engine.stop();
            });

            e.start().await;
            // eprintln!("log = {}", log);
            println!("Engine stopped");
        })
    }
}
