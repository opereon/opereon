#![feature(async_closure, termination_trait_lib)]

#[macro_use]
extern crate serde_derive;

use async_trait::async_trait;
use futures::prelude::*;
use kg_utils::collections::LinkedHashMap;
use kg_utils::sync::*;
use std::collections::VecDeque;
use std::future::Future;
use std::ops::Deref;
use std::pin::Pin;
use std::process::Termination;
use std::task::{Context, Poll, Waker};
use std::time::Duration;
use tokio::prelude::*;
use tokio::time::Interval;
use uuid::Uuid;

mod operation;
mod progress;

use operation::*;
use progress::*;

struct Operations<T: std::clone::Clone + 'static> {
    operation_queue1: VecDeque<OperationRef<T>>,
    operation_queue2: VecDeque<OperationRef<T>>,
    operations: LinkedHashMap<Uuid, OperationRef<T>>,
}

impl<T: std::clone::Clone + 'static> Operations<T> {
    fn new() -> Operations<T> {
        Operations {
            operation_queue1: VecDeque::new(),
            operation_queue2: VecDeque::new(),
            operations: LinkedHashMap::new(),
        }
    }

    fn add_operation(&mut self, op: OperationRef<T>) {
        self.operation_queue1.push_back(op.clone());
        self.operations.insert(op.id(), op);
    }

    fn remove_operation(&mut self, op: &OperationRef<T>) {
        self.operations.remove(&op.id());
    }

    fn swap_queues(&mut self) {
        std::mem::swap(&mut self.operation_queue1, &mut self.operation_queue2);
    }
}

struct Core<T: std::clone::Clone + 'static> {
    waker: Option<Waker>,
    progress_callback: Option<Box<dyn FnMut(&EngineRef<T>, &OperationRef<T>)>>,
}

impl<T: std::clone::Clone + 'static> Core<T> {
    fn new() -> Core<T> {
        Core {
            waker: None,
            progress_callback: None,
        }
    }

    fn wake(&mut self) {
        if let Some(w) = self.waker.take() {
            w.wake();
        }
    }

    fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }
}

struct Services {}

impl Services {
    fn new() -> Services {
        Services {}
    }
}

#[derive(Clone)]
pub struct EngineRef<T: std::clone::Clone + 'static> {
    operations: SyncRef<Operations<T>>,
    core: SyncRef<Core<T>>,
    services: SyncRef<Services>,
}

impl<T: std::clone::Clone + 'static> EngineRef<T> {
    pub fn new() -> EngineRef<T> {
        EngineRef {
            operations: SyncRef::new(Operations::new()),
            core: SyncRef::new(Core::new()),
            services: SyncRef::new(Services::new()),
        }
    }

    pub fn run_with(&self, f: impl FnOnce(EngineRef<T>) -> ()) -> EngineResult {
        let mut runtime = tokio::runtime::Builder::new()
            .enable_all()
            .threaded_scheduler()
            .thread_name("engine")
            .build()
            .unwrap();
        let e = self.clone();
        runtime.block_on(async {
            f(e);
            self.main_task().await
        })
    }

    pub fn run(&self) -> EngineResult {
        self.run_with(|_| {})
    }

    pub fn operations(&self) -> SyncRefMapReadGuard<LinkedHashMap<Uuid, OperationRef<T>>> {
        let ops = self.operations.read();
        SyncRefReadGuard::map(ops, |o| &o.operations)
    }

    fn main_task(&self) -> EngineMainTask<T> {
        EngineMainTask {
            engine: self.clone(),
        }
    }

    fn set_waker(&self, waker: Waker) {
        self.core.write().set_waker(waker);
    }

    pub fn enqueue_operation(&self, operation: OperationRef<T>) {
        self.operations.write().add_operation(operation.clone());
        self.core.write().wake();
    }

    fn finish_operation(&self, operation: &OperationRef<T>) {
        if operation.read().parent().is_some() {
        } else {
            self.operations.write().remove_operation(operation);
        }
        self.core.write().wake();
    }

    pub fn register_progress_cb<F: FnMut(&EngineRef<T>, &OperationRef<T>) + 'static>(
        &self,
        callback: F,
    ) {
        self.core.write().progress_callback = Some(Box::new(callback));
    }

    fn notify_progress(&self, operation: &OperationRef<T>) {
        if let Some(ref mut cb) = self.core.write().progress_callback {
            cb(&self, operation);
        }
    }
}

pub struct EngineResult {
    code: i32,
}

impl Termination for EngineResult {
    fn report(self) -> i32 {
        self.code
    }
}

struct EngineMainTask<T: std::clone::Clone + 'static> {
    engine: EngineRef<T>,
}

impl<T: std::clone::Clone + 'static> Future for EngineMainTask<T> {
    type Output = EngineResult;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        //println!("engine task poll: {}", std::thread::current().name().unwrap());

        self.engine.set_waker(cx.waker().clone());

        if !self.engine.operations.read().operation_queue1.is_empty() {
            let mut ops = self.engine.operations.write();
            while let Some(mut op) = ops.operation_queue1.pop_front() {
                let op_impl = op.take_op_impl().unwrap();
                tokio::spawn(get_operation_fut(self.engine.clone(), op, op_impl));
            }
            ops.swap_queues();
        }

        if self.engine.operations.read().operations.is_empty() {
            Poll::Ready(EngineResult { code: 0 })
        } else {
            Poll::Pending
        }
    }
}

async fn get_operation_fut<T: std::clone::Clone + 'static>(
    engine: EngineRef<T>,
    operation: OperationRef<T>,
    mut op_impl: Box<dyn OperationImpl<T>>,
) {
    // TODO ws errors
    op_impl.init(&engine, &operation).await.unwrap();

    while !operation.write().progress().is_done() {
        // TODO ws errors
        let u = op_impl.next_progress(&engine, &operation).await.unwrap();
        operation.write().progress_mut().update(u);
        engine.notify_progress(&operation);
    }
    // TODO ws errors
    let res = op_impl.done(&engine, &operation).await.unwrap();

    operation.write().set_outcome(res);

    engine.finish_operation(&operation);
}

#[async_trait]
pub trait OperationImpl<T: std::clone::Clone + 'static>: Send {
    async fn init(&mut self, _engine: &EngineRef<T>, _operation: &OperationRef<T>) -> Result<(), ()> {
        Ok(())
    }

    async fn next_progress(
        &mut self,
        engine: &EngineRef<T>,
        operation: &OperationRef<T>,
    ) -> Result<ProgressUpdate, ()>;

    async fn done(&mut self, _engine: &EngineRef<T>, _operation: &OperationRef<T>) -> Result<T, ()>;

}

#[cfg(test)]
mod tests {
    use super::*;

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
        ) -> Result<ProgressUpdate, ()> {
            //println!("progress: {}", operation.read().name);
            if self.count > 0 {
                self.count -= 1;
                self.interval.tick().await;

                Ok(ProgressUpdate::new((5.0 - self.count as f64) * 20.0))
            } else {
                Ok(ProgressUpdate::done())
            }
        }

        async fn done(&mut self, _engine: &EngineRef<OutputType>, _operation: &OperationRef<OutputType>) -> Result<OutputType, ()> {
            let delay = tokio::time::delay_for(Duration::from_secs(2));
            println!("Some long running cleanup code....");
            delay.await;
            println!("cleanup finished....");
            Ok("()".into())
        }
    }

    fn print_progress<T: std::clone::Clone + 'static>(e: &EngineRef<T>, first: bool) {
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

        engine.register_progress_cb(|e, o| {
            print_progress(e, false);
        });

        print_progress(&engine, true);

        engine.run_with(|engine| {
            engine.enqueue_operation(OperationRef::new("ddd1", TestOp::new()));
            engine.enqueue_operation(OperationRef::new("ddd2", TestOp::new()));
            engine.enqueue_operation(OperationRef::new("ddd3", TestOp::new()));
            engine.enqueue_operation(OperationRef::new("ddd4", TestOp::new()));
        });
    }
}
