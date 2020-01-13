#![feature(async_closure, termination_trait_lib)]

#[macro_use]
extern crate serde_derive;

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

struct Services {

}

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
                let op_task = OperationTask::new(
                    self.engine.clone(),
                    op,
                    TestOp::new());
                tokio::spawn(op_task);
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

struct OperationTask {
    engine: EngineRef,
    operation: OperationRef,
    op_state: OperationState,
    op_impl: Pin<Box<dyn OperationImpl>>,
}

impl OperationTask {
    fn new(engine: EngineRef, operation: OperationRef, op_impl: Box<dyn OperationImpl>) -> OperationTask {
        OperationTask {
            engine,
            operation,
            op_state: OperationState::Init,
            op_impl: op_impl.into(),
        }
    }
}

impl Future for OperationTask {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use std::mem::transmute;

        //println!("operation task poll: {} [{}]", self.operation.read().name, std::thread::current().name().unwrap());

        // It is safe to take references below, because only `self.op_impl` is borrowed as mutable
        // transmuting lifetimes to silence the borrow checker
        let engine = unsafe { transmute::<&'_ EngineRef, &'_ EngineRef>(&self.engine) };
        let operation = unsafe { transmute::<&'_ OperationRef, &'_ OperationRef>(&self.operation) };

        let progress_counter1;

        {
            let mut o = operation.write();
            o.set_waker(cx.waker().clone());
            progress_counter1 = o.progress.counter();
        }

        //TODO: unwind panics
        let p = match self.op_state {
            OperationState::Init => match self.op_impl.as_mut().poll_init(cx, engine, operation) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => {
                    self.op_state = OperationState::Progress;
                    operation.write().wake();
                    Poll::Pending
                }
            },
            OperationState::Progress => match self.op_impl.as_mut().poll_progress(cx, engine, operation) {
                Poll::Pending => {
                    let mut o = operation.write();
                    o.wake();
                    Poll::Pending
                },
                Poll::Ready(u) => {
                    let mut o = operation.write();
                    o.progress.update(u);
                    o.wake();
                    if o.progress.is_done() {
                        self.op_state = OperationState::Done;
                    }
                    Poll::Pending
                }
            },
            OperationState::Done => match self.op_impl.as_mut().poll_done(cx, engine, operation) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => {
                    engine.finish_operation(operation);
                    Poll::Ready(())
                }
            },
            OperationState::Cancel => match self.op_impl.as_mut().poll_cancel(cx, engine, operation) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => {
                    engine.finish_operation(operation);
                    Poll::Ready(())
                }
            },
        };

        let progress_counter2 = operation.read().progress.counter();

        if progress_counter1 != progress_counter2 {
            engine.notify_progress(operation);
        }

        p
    }
}


trait OperationImpl: Send {
    fn poll_init(self: Pin<&mut Self>, _cx: &mut Context, _engine: &EngineRef, _operation: &OperationRef) -> Poll<()> {
        Poll::Ready(())
    }

    fn poll_progress(self: Pin<&mut Self>, cx: &mut Context, engine: &EngineRef, operation: &OperationRef) -> Poll<ProgressUpdate>;

    fn poll_done(self: Pin<&mut Self>, _cx: &mut Context, _engine: &EngineRef, _operation: &OperationRef) -> Poll<()> {
        Poll::Ready(())
    }

    fn poll_cancel(self: Pin<&mut Self>, _cx: &mut Context, _engine: &EngineRef, _operation: &OperationRef) -> Poll<()> {
        Poll::Ready(())
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

impl OperationImpl for TestOp {
    fn poll_progress(mut self: Pin<&mut Self>, cx: &mut Context, _engine: &EngineRef, _operation: &OperationRef) -> Poll<ProgressUpdate> {
        //println!("progress: {}", operation.read().name);
        if self.count > 0 {
            let interval = Pin::new(&mut self.interval);
            match interval.poll_next(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => {
                    self.count -= 1;
                    Poll::Ready(ProgressUpdate::new((5.0 - self.count as f64) * 20.0))
                }
            }
        } else {
            Poll::Ready(ProgressUpdate::done())
        }
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
