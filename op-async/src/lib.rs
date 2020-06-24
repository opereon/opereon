#![feature(async_closure, termination_trait_lib)]

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate kg_diag_derive;
#[macro_use]
extern crate kg_display_derive;

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

pub mod operation;
pub mod progress;

pub use operation::{OperationError, OperationErrorDetail, OperationImpl, OperationRef};
pub use progress::ProgressUpdate;

use kg_diag::BasicDiag;
use operation::*;
use progress::*;
use tokio::sync::oneshot::Receiver;
use tokio::sync::oneshot::error::RecvError;
use tokio::runtime::Runtime;

struct Operations<T: Clone + 'static> {
    operation_queue1: VecDeque<OperationRef<T>>,
    operation_queue2: VecDeque<OperationRef<T>>,
    operations: LinkedHashMap<Uuid, OperationRef<T>>,
}

impl<T: Clone + 'static> Operations<T> {
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

struct Core<T: Clone + 'static> {
    waker: Option<Waker>,
    progress_callback: Option<Box<dyn FnMut(&EngineRef<T>, &OperationRef<T>)>>,
    stopped: bool
}

impl<T: Clone + 'static> Core<T> {
    fn new() -> Core<T> {
        Core {
            waker: None,
            progress_callback: None,
            stopped: false,
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

    fn set_stopped(&mut self, stopped: bool) {
        self.stopped = stopped
    }

    fn stopped(&self) -> bool {
        self.stopped
    }
}

struct Services {}

impl Services {
    fn new() -> Services {
        Services {}
    }
}

#[derive(Clone)]
pub struct EngineRef<T: Clone + 'static> {
    operations: SyncRef<Operations<T>>,
    core: SyncRef<Core<T>>,
    services: SyncRef<Services>,
}

impl<T: Clone + 'static> EngineRef<T> {
    pub fn new() -> EngineRef<T> {
        EngineRef {
            operations: SyncRef::new(Operations::new()),
            core: SyncRef::new(Core::new()),
            services: SyncRef::new(Services::new()),
        }
    }

    pub fn build_runtime() -> Runtime {
        let runtime = tokio::runtime::Builder::new()
            .enable_all()
            .threaded_scheduler()
            .thread_name("engine")
            .build()
            .unwrap();
        runtime
    }

    // /// This method is necessary if we want to create OperationImpl instances in context of tokio runtime
    // pub fn run_with(&self, f: impl FnOnce(EngineRef<T>) -> ()) -> EngineResult {
    //     let mut runtime = Self::build_runtime();
    //     let e = self.clone();
    //     runtime.block_on(async {
    //         f(e);
    //         self.main_task().await
    //     })
    // }

    // pub fn run(&self) -> EngineResult {
    //     self.run_with(|_| {})
    // }

    pub async fn start(&self)  -> EngineResult {
        self.main_task().await
    }

    pub fn stop(&self) {
        self.core.write().set_stopped(true);
        self.core.write().wake();
    }

    fn stopped(&self) -> bool {
        self.core.read().stopped()
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

    pub fn enqueue_operation(&self, operation: OperationRef<T>) -> Receiver<()>{
        let (sender, receiver) = tokio::sync::oneshot::channel();
        operation.write().set_done_sender(sender);
        self.operations.write().add_operation(operation.clone());
        self.core.write().wake();
        receiver
    }

    pub fn enqueue_with_res(&self, operation: OperationRef<T>) -> impl Future<Output=OperationResult<T>> {
        let receiver = self.enqueue_operation(operation.clone());

        async move {
            match receiver.await {
                Ok(_) => {
                    // outcome is always set after operation completion
                    operation.write().take_outcome().unwrap()
                },
                Err(_) => {
                    // should never happen since only way to drop sender is:
                    // - complete operation (notification sent before drop)
                    // - drop entire operation (engine will never drop operation without completion)
                    unreachable!()
                },
            }
        }
    }

    fn finish_operation(&self, operation: &OperationRef<T>, res: OperationResult<T>) {
        operation.write().set_outcome(res);
        // this is safe since operations scheduled with `enqueue_operation` always have `done_sender`
        let sender = operation.write().take_done_sender().unwrap();
        match sender.send(()) {
            Ok(_) => {
            },
            Err(_) => {
                // nothing to do here. Its totally ok to have receiver deallocated (Fire and forget scenario)
            },
        };

        if operation.read().parent().is_some() {
            // TODO ws what to do here?
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

struct EngineMainTask<T: Clone + 'static> {
    engine: EngineRef<T>,
}

impl<T: Clone + 'static> Future for EngineMainTask<T> {
    type Output = EngineResult;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        //println!("engine task poll: {}", std::thread::current().name().unwrap());

        self.engine.set_waker(cx.waker().clone());

        if !self.engine.stopped() {
            if !self.engine.operations.read().operation_queue1.is_empty() {
                let mut ops = self.engine.operations.write();
                while let Some(mut op) = ops.operation_queue1.pop_front() {
                    let op_impl = op.take_op_impl().unwrap();
                    tokio::spawn(get_operation_fut(self.engine.clone(), op, op_impl));
                }
                ops.swap_queues();
            }
            Poll::Pending
        } else {
            Poll::Ready(EngineResult { code: 0 })
        }
    }
}

async fn get_operation_fut<T: Clone + 'static>(
    engine: EngineRef<T>,
    operation: OperationRef<T>,
    mut op_impl: Box<dyn OperationImpl<T>>,
) {
    let o = operation.clone();
    let e = engine.clone();
    let mut inner = async move || {
        op_impl.init(&engine, &operation).await?;

        while !operation.write().progress().is_done() {
            let u = op_impl.next_progress(&engine, &operation).await?;
            operation.write().progress_mut().update(u);
            engine.notify_progress(&operation);
        }
        op_impl.done(&engine, &operation).await
    };

    let out = inner().await;
    e.finish_operation(&o, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::*;

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

        engine.register_progress_cb(|e, o| {
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
