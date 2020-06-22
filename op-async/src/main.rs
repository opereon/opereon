#![feature(async_closure, termination_trait_lib)]

#[macro_use]
extern crate serde_derive;

use async_trait::async_trait;
use std::collections::VecDeque;
use std::future::Future;
use std::task::{Context, Poll, Waker};
use std::pin::Pin;
use std::process::Termination;
use std::ops::Deref;
use std::time::Duration;
use futures::prelude::*;
use tokio::prelude::*;
use tokio::time::Interval;
use uuid::Uuid;
use kg_utils::collections::LinkedHashMap;
use kg_utils::sync::*;

mod progress;

use progress::*;


struct Operation {
    id: Uuid,
    parent: Uuid,
    operations: Vec<Uuid>,
    name: String,
    progress: Progress,
    waker: Option<Waker>,
    op_state: OperationState,
}

impl Operation {
    fn new<S: Into<String>>(name: S) -> Operation {
        Operation {
            id: Uuid::new_v4(),
            parent: Uuid::nil(),
            operations: Vec::new(),
            name: name.into(),
            progress: Progress::default(),
            waker: None,
            op_state: OperationState::Init
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

    fn parent(&self) -> Option<Uuid> {
        if self.parent.is_nil() {
            None
        } else {
            Some(self.parent)
        }
    }
}

#[derive(Clone)]
struct OperationRef(SyncRef<Operation>);

impl Deref for OperationRef {
    type Target = SyncRef<Operation>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl OperationRef {
    fn new<S: Into<String>>(name: S) -> OperationRef {
        OperationRef(SyncRef::new(Operation::new(name)))
    }

    fn id(&self) -> Uuid {
        self.0.read().id
    }

    fn set_waker(&self, waker: Waker) {
        self.write().set_waker(waker);
    }
}

struct Operations {
    operation_queue1: VecDeque<OperationRef>,
    operation_queue2: VecDeque<OperationRef>,
    operations: LinkedHashMap<Uuid, OperationRef>,
}

impl Operations {
    fn new() -> Operations {
        Operations {
            operation_queue1: VecDeque::new(),
            operation_queue2: VecDeque::new(),
            operations: LinkedHashMap::new(),
        }
    }

    fn add_operation(&mut self, op: OperationRef) {
        self.operation_queue1.push_back(op.clone());
        self.operations.insert(op.id(), op);
    }

    fn remove_operation(&mut self, op: &OperationRef) {
        self.operations.remove(&op.id());
    }

    fn swap_queues(&mut self) {
        std::mem::swap(&mut self.operation_queue1, &mut self.operation_queue2);
    }
}

struct Core {
    waker: Option<Waker>,
    progress_callback: Option<Box<dyn FnMut(&EngineRef, &OperationRef)>>,
}

impl Core {
    fn new() -> Core {
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
struct EngineRef {
    operations: SyncRef<Operations>,
    core: SyncRef<Core>,
    services: SyncRef<Services>,
}

impl EngineRef {
    pub fn new() -> EngineRef {
        EngineRef {
            operations: SyncRef::new(Operations::new()),
            core: SyncRef::new(Core::new()),
            services: SyncRef::new(Services::new()),
        }
    }

    pub fn run(&self) -> EngineResult {
        let mut runtime = tokio::runtime::Builder::new()
            .enable_all()
            .threaded_scheduler()
            .thread_name("engine")
            .build()
            .unwrap();
        runtime.block_on(async {
            self.main_task().await
        })
    }

    pub fn operations(&self) -> SyncRefMapReadGuard<LinkedHashMap<Uuid, OperationRef>> {
        let ops = self.operations.read();
        SyncRefReadGuard::map(ops, |o| &o.operations)
    }

    fn main_task(&self) -> EngineMainTask {
        EngineMainTask {
            engine: self.clone(),
        }
    }

    fn set_waker(&self, waker: Waker) {
        self.core.write().set_waker(waker);
    }

    pub fn enqueue_operation(&self, operation: OperationRef) {
        self.operations.write().add_operation(operation.clone());
        self.core.write().wake();
    }

    fn finish_operation(&self, operation: &OperationRef) {
        if operation.read().parent().is_some() {

        } else {
            self.operations.write().remove_operation(operation);
        }
        self.core.write().wake();
    }

    pub fn register_progress_cb<F: FnMut(&EngineRef, &OperationRef) + 'static>(&self, callback: F) {
        self.core.write().progress_callback = Some(Box::new(callback));
    }

    fn notify_progress(&self, operation: &OperationRef) {
        if let Some(ref mut cb) = self.core.write().progress_callback {
            cb(&self, operation);
        }
    }
}


struct EngineResult {
    code: i32,
}

impl Termination for EngineResult {
    fn report(self) -> i32 {
        self.code
    }
}


struct EngineMainTask {
    engine: EngineRef,
}

impl Future for EngineMainTask {
    type Output = EngineResult;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        //println!("engine task poll: {}", std::thread::current().name().unwrap());

        self.engine.set_waker(cx.waker().clone());

        if !self.engine.operations.read().operation_queue1.is_empty() {
            let mut ops = self.engine.operations.write();
            while let Some(op) = ops.operation_queue1.pop_front() {
                tokio::spawn(get_operation_fut(self.engine.clone(), op, TestOp::new()));
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


#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum OperationState {
    Init,
    Progress,
    Done,
    Cancel,
}

async fn get_operation_fut(engine: EngineRef, operation: OperationRef, mut op_impl: Box<dyn OperationImpl>) {
    op_impl.init(&engine, &operation).await;

    while !operation.write().progress.is_done() {
        let u = op_impl.next_progress(&engine, &operation).await;
        operation.write().progress.update(u);
        engine.notify_progress(&operation);
    }

    op_impl.done(&engine, &operation).await;

    engine.finish_operation(&operation);
}

#[async_trait]
trait OperationImpl: Send {
    async fn init(&mut self, _engine: &EngineRef, _operation: &OperationRef) -> () {
        ()
    }

    async fn next_progress(&mut self, engine: &EngineRef, operation: &OperationRef) -> ProgressUpdate;

    async fn done(&mut self, _engine: &EngineRef, _operation: &OperationRef) -> () {
        ()
    }

    async fn cancel(&mut self, _engine: &EngineRef, _operation: &OperationRef) -> () {
        ()
    }
}

struct TestOp {
    interval: Interval,
    count: usize,
}

impl TestOp {
    fn new() -> Box<Self> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        Box::new(TestOp {
            interval: tokio::time::interval(Duration::from_secs(rng.gen_range(1, 3))),
            count: rng.gen_range(1, 5),
        })
    }
}

#[async_trait]
impl OperationImpl for TestOp {
    async fn next_progress(&mut self, _engine: &EngineRef, _operation: &OperationRef) -> ProgressUpdate {
        //println!("progress: {}", operation.read().name);
        if self.count > 0 {
            self.count -= 1;
            self.interval.tick().await;

            ProgressUpdate::new((5.0 - self.count as f64) * 20.0)
        } else {
            ProgressUpdate::done()
        }
    }

    async fn done(&mut self, _engine: &EngineRef, _operation: &OperationRef) -> () {
        let delay = tokio::time::delay_for(Duration::from_secs(2));
        println!("Some long running cleanup code....");
        delay.await;
        println!("cleanup finished....");
    }

}

fn print_progress(e: &EngineRef, first: bool) {
    use std::fmt::Write;

    let mut s = String::new();
    write!(s, "---\n").unwrap();
    for o in e.operations().values() {
        let o = o.read();
        write!(s, "operation: {} -> {}\n", o.name, o.progress).unwrap();
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

fn main() -> EngineResult {
    let engine = EngineRef::new();

    engine.enqueue_operation(OperationRef::new("ddd1"));
    engine.enqueue_operation(OperationRef::new("ddd2"));
    engine.enqueue_operation(OperationRef::new("ddd3"));
    engine.enqueue_operation(OperationRef::new("ddd4"));

    engine.register_progress_cb(|e, o| {
        print_progress(e, false);
    });

    print_progress(&engine, true);

    engine.run()
}
