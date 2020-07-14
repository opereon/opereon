use crate::ops::combinators::handle_cancel;
use crate::outcome::Outcome;
use async_trait::*;
use op_engine::operation::OperationResult;
use op_engine::progress::{Progress, Unit};
use op_engine::{EngineRef, OperationError, OperationImpl, OperationRef, ProgressUpdate};
use tokio::task::JoinHandle;

pub struct SequenceOperation {
    ops: Vec<OperationRef<Outcome>>,
    current_step: usize,
    outcomes: Vec<Outcome>,
}

impl SequenceOperation {
    pub fn new(ops: Vec<OperationRef<Outcome>>) -> Self {
        SequenceOperation {
            outcomes: Vec::with_capacity(ops.len()),
            ops,
            current_step: 0,
        }
    }
}

#[async_trait]
impl OperationImpl<Outcome> for SequenceOperation {
    async fn init(
        &mut self,
        _engine: &EngineRef<Outcome>,
        operation: &OperationRef<Outcome>,
    ) -> OperationResult<()> {
        handle_cancel(self.ops.clone(), operation);

        *operation.write().progress_mut() = Progress::new(0., self.ops.len() as f64, Unit::Scalar);
        Ok(())
    }

    async fn next_progress(
        &mut self,
        engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<ProgressUpdate> {
        if self.current_step == self.ops.len() {
            return Ok(ProgressUpdate::done());
        }

        let op = self.ops[self.current_step].clone();
        let out = engine.enqueue_with_res(op).await?;
        self.outcomes.push(out);
        self.current_step += 1;
        let pu = ProgressUpdate::new(self.current_step as f64);
        Ok(pu)
    }

    async fn done(
        &mut self,
        _engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<Outcome> {
        Ok(Outcome::Many(std::mem::replace(
            &mut self.outcomes,
            Vec::new(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::Outcome;
    use kg_diag::IntoDiagRes;
    use kg_diag::Severity;
    use op_engine::operation::OperationResult;
    use op_engine::{EngineRef, OperationImpl, OperationRef};
    use tokio::time::Duration;

    use crate::ops::combinators::parallel::ParallelOperation;
    use async_trait::*;
    use kg_diag::io::ResultExt;

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
            OperationRef::new("test_op", op_impl)
        }
        pub fn new_op_fail(duration: u64) -> OperationRef<Outcome> {
            let op_impl = TestOp {
                should_fail: true,
                duration,
            };
            OperationRef::new("test_op", op_impl)
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
    fn sequence_operation_test() {
        let engine: EngineRef<Outcome> = EngineRef::default();
        let mut rt = EngineRef::<()>::build_runtime();

        let ops = vec![TestOp::new_op(1), TestOp::new_op(1), TestOp::new_op(1)];

        let op_impl = SequenceOperation::new(ops);
        let op = OperationRef::new("parallel_operation", op_impl);

        rt.block_on(async move {
            let e = engine.clone();
            engine.register_progress_cb(|_e, o| eprintln!("progress: {}", o.read().progress()));

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
}
