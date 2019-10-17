#![feature(async_closure, termination_trait_lib)]

#[macro_use]
extern crate serde_derive;

use std::collections::VecDeque;

use tokio::prelude::*;
use tokio::runtime;
use uuid::Uuid;

use kg_utils::collections::LinkedHashMap;
use kg_utils::sync::*;
use std::task::{Context, Poll, Waker};
use std::pin::Pin;
use std::process::Termination;
use std::ops::Deref;
use tokio::timer::Interval;
use std::time::Duration;

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
    progress_callbacks: Vec<Box<dyn FnMut(&EngineRef, &OperationRef)>>,
}

impl Core {
    fn new() -> Core {
        Core {
            waker: None,
            progress_callbacks: Vec::new(),
        }
    }

    fn wake(&mut self) {
        if let Some(w) = self.waker.take() {
            w.wake();
        }
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
    fn new() -> EngineRef {
        EngineRef {
            operations: SyncRef::new(Operations::new()),
            core: SyncRef::new(Core::new()),
            services: SyncRef::new(Services::new()),
        }
    }

    fn main_task(&self) -> EngineMainTask {
        EngineMainTask {
            engine: self.clone(),
        }
    }

    fn operations(&self) -> SyncRefMapReadGuard<LinkedHashMap<Uuid, OperationRef>> {
        let ops = self.operations.read();
        SyncRefReadGuard::map(ops, |o| &o.operations)
    }

    fn set_waker(&self, waker: Waker) {
        self.core.write().waker = Some(waker);
    }

    #[inline]
    fn enqueue_operation(&self, op: OperationRef) {
        self.operations.write().add_operation(op.clone());
        self.core.write().wake();
    }

    #[inline]
    fn finish_operation(&self, op: &OperationRef) {
        self.operations.write().remove_operation(op);
        self.core.write().wake();
    }

    #[inline]
    fn register_progress_cb<F: FnMut(&EngineRef, &OperationRef) + 'static>(&self, callback: F) {
        self.core.write().progress_callbacks.push(Box::new(callback));
    }

    #[inline]
    fn notify_progress(&self, operation: &OperationRef) {
        for cb in self.core.write().progress_callbacks.iter_mut() {
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
                let task = OperationTask::new(
                    self.engine.clone(),
                    op,
                    TestOp::new());
                tokio::spawn(task);
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
}

struct OperationTask {
    engine: EngineRef,
    operation: OperationRef,
    op_state: OperationState,
    op_impl: Pin<Box<dyn OperationImpl>>,
}

impl OperationTask {
    fn new(engine: EngineRef, operation: OperationRef, op_impl: Pin<Box<dyn OperationImpl>>) -> OperationTask {
        OperationTask {
            engine,
            operation,
            op_state: OperationState::Init,
            op_impl,
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

        let progress_counter;

        {
            let mut o = operation.write();
            o.waker = Some(cx.waker().clone());
            progress_counter = o.progress.counter();
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
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => {
                    self.op_state = OperationState::Done;
                    let mut o = operation.write();
                    o.progress.set_value_done();
                    o.wake();
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
        };

        let progress_counter2 = operation.read().progress.counter();

        if progress_counter != progress_counter2 {
            engine.notify_progress(operation);
        }

        p
    }
}


trait OperationImpl: Send {
    fn poll_init(self: Pin<&mut Self>, cx: &mut Context, engine: &EngineRef, operation: &OperationRef) -> Poll<()>;
    fn poll_progress(self: Pin<&mut Self>, cx: &mut Context, engine: &EngineRef, operation: &OperationRef) -> Poll<()>;
    fn poll_done(self: Pin<&mut Self>, cx: &mut Context, engine: &EngineRef, operation: &OperationRef) -> Poll<()>;
}

struct TestOp {
    interval: Interval,
    count: usize,
}

impl TestOp {
    fn new() -> Pin<Box<Self>> {
        Box::pin(TestOp {
            interval: Interval::new_interval(Duration::from_secs(1)),
            count: 5,
        })
    }
}

impl OperationImpl for TestOp {
    fn poll_init(mut self: Pin<&mut Self>, cx: &mut Context, engine: &EngineRef, operation: &OperationRef) -> Poll<()> {
        //println!("init: {}", operation.read().name);
        self.interval.poll_next_unpin(cx).map(|_| ())
    }

    fn poll_progress(mut self: Pin<&mut Self>, cx: &mut Context, engine: &EngineRef, operation: &OperationRef) -> Poll<()> {
        //println!("progress: {}", operation.read().name);
        if self.count > 0 {
            match self.interval.poll_next_unpin(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(_) => {
                    self.count -= 1;
                    {
                        let mut o = operation.write();
                        o.progress.set_value((5.0 - self.count as f64) * 20.0);
                        o.wake();
                    }
                    Poll::Pending
                }
            }
        } else {
            Poll::Ready(())
        }
    }

    fn poll_done(mut self: Pin<&mut Self>, cx: &mut Context, engine: &EngineRef, operation: &OperationRef) -> Poll<()> {
        //println!("done: {}", operation.read().name);
        self.interval.poll_next_unpin(cx).map(|_| ())
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

    let runtime = {
        let mut builder = runtime::Builder::new();
        builder.name_prefix("op-").build().unwrap()
    };

    engine.enqueue_operation(OperationRef::new("ddd1"));
    engine.enqueue_operation(OperationRef::new("ddd2"));
    engine.enqueue_operation(OperationRef::new("ddd3"));
    engine.enqueue_operation(OperationRef::new("ddd4"));
    engine.enqueue_operation(OperationRef::new("ddd5"));

    print_progress(&engine, true);

    engine.register_progress_cb(|e, o| {
        print_progress(e, false);
    });

    runtime.block_on(async {
        engine.main_task().await
    })
}
