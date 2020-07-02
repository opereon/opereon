
#![feature(async_closure, termination_trait_lib)]

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate kg_diag_derive;
#[macro_use]
extern crate kg_display_derive;

pub mod engine;
pub mod operation;
pub mod progress;

pub use operation::{OperationError, OperationErrorDetail, OperationImpl, OperationRef};
pub use progress::{ProgressUpdate};
pub use engine::{EngineRef, EngineResult};

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::*;
    use crate::operation::OperationResult;
    use tokio::time::{Duration, Interval};

    struct TestOp {
        interval: Interval,
        count: usize,
    }

    impl TestOp {
        fn new() -> Self {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            TestOp {
                interval: tokio::time::interval(Duration::from_secs(rng.gen_range(1, 3))),
                count: rng.gen_range(1, 5),
            }
        }
    }

    type OutputType = String;

    #[async_trait]
    impl OperationImpl<OutputType> for TestOp {
        async fn next_progress(
            &mut self,
            _engine: &EngineRef<OutputType>,
            _operation: &OperationRef<OutputType>,
        ) -> OperationResult<ProgressUpdate> {
            //println!("progress: {}", operation.read().name);
            if self.count > 0 {
                self.count -= 1;
                self.interval.tick().await;

                Ok(ProgressUpdate::new((5.0 - self.count as f64) * 20.0))
            } else {
                Ok(ProgressUpdate::done())
            }
        }

        async fn done(
            &mut self,
            _engine: &EngineRef<OutputType>,
            _operation: &OperationRef<OutputType>,
        ) -> OperationResult<OutputType> {
            let delay = tokio::time::delay_for(Duration::from_secs(2));
            println!("Some long running cleanup code....");
            delay.await;
            println!("cleanup finished....");
            Ok("()".into())
        }
    }

    fn print_progress<T: Clone + 'static>(e: &EngineRef<T>, first: bool) {
        use std::fmt::Write;

        let mut s = String::new();
        write!(s, "---\n").unwrap();
        for o in e.operations().values() {
            let o = o.read();
            write!(s, "operation: {} -> {}\n", o.name(), o.progress()).unwrap();
        }
        write!(s, "===\n").unwrap();
        print_output(&s, first);
    }

    fn print_output(output: &str, first: bool) {
        if !first {
            let lines = output.lines().count();
            print!("\x1B[{}A", lines);
        }
        print!("{}", output);
    }

    #[test]
    fn test_operation() {
        let engine: EngineRef<String> = EngineRef::new();

        engine.register_progress_cb(|e, _o| {
            print_progress(e, false);
        });

        print_progress(&engine, true);

        let mut rt = EngineRef::<()>::build_runtime();

        rt.block_on(async move {
            let e =engine.clone();
            tokio::spawn(async move {
                engine.enqueue_operation(OperationRef::new("ddd1", TestOp::new()));
                engine.enqueue_operation(OperationRef::new("ddd2", TestOp::new()));
                engine.enqueue_operation(OperationRef::new("ddd3", TestOp::new()));
                engine.enqueue_with_res(OperationRef::new("ddd4", TestOp::new())).await;
                engine.stop()
            });
            e.start().await;
        });
    }
}
