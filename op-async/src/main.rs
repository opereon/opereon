#![feature(async_closure, termination_trait_lib)]

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
    progress: f64,
    waker: Option<Waker>,
}

impl Operation {
    fn new<S: Into<String>>(name: S) -> Operation {
        Operation {
            id: Uuid::new_v4(),
            parent: Uuid::nil(),
            operations: Vec::new(),
            name: name.into(),
            progress: 0.0,
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

struct Engine {
    operation_queue1: VecDeque<OperationRef>,
    operation_queue2: VecDeque<OperationRef>,
    operations: LinkedHashMap<Uuid, OperationRef>,
    waker: Option<Waker>,
    progress_callbacks: Vec<Box<dyn Fn(OperationRef)>>,
}

impl Engine {
    fn new() -> Engine {
        Engine {
            operation_queue1: VecDeque::new(),
            operation_queue2: VecDeque::new(),
            operations: LinkedHashMap::new(),
            waker: None,
            progress_callbacks: Vec::new(),
        }
    }

    fn wake(&mut self) {
        if let Some(w) = self.waker.take() {
            w.wake();
        }
    }

    fn enqueue_op(&mut self, op: OperationRef) {
        self.operation_queue1.push_back(op.clone());
        self.operations.insert(op.id(), op);
        self.wake();
    }

    fn finish_op(&mut self, op: &OperationRef) {
        self.operations.remove(&op.id());
        self.wake();
    }
}

#[derive(Clone)]
struct EngineRef(SyncRef<Engine>);

impl Deref for EngineRef {
    type Target = SyncRef<Engine>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl EngineRef {
    fn new(engine: Engine) -> EngineRef {
        EngineRef(SyncRef::new(engine))
    }

    fn main_task(&self) -> EngineMainTask {
        EngineMainTask {
            engine: self.clone(),
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
        println!("engine task poll: {}", std::thread::current().name().unwrap());

        {
            let mut e = self.engine.write();
            e.waker = Some(cx.waker().clone());
        }

        if !self.engine.read().operation_queue1.is_empty() {
            let mut e = self.engine.write();
            while let Some(op) = e.operation_queue1.pop_front() {
                let task = OperationTask::new(self.engine.clone(), op);
                tokio::spawn(task);
            }
        }

        if self.engine.read().operations.is_empty() {
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
    op_impl: Box<dyn OperationImpl>,
}

impl OperationTask {
    fn new(engine: EngineRef, operation: OperationRef, op_impl: Box<dyn OperationImpl>) -> OperationTask {
        OperationTask {
            engine,
            operation,
            op_state: OperationState::Init,
            op_impl,
//            interval: Interval::new_interval(Duration::from_secs(3)),
        }
    }
}

impl Future for OperationTask {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        println!("operation task poll: {} [{}]", self.operation.read().name, std::thread::current().name().unwrap());

        {
            let mut o = self.operation.write();
            o.waker = Some(cx.waker().clone());
        }

        match self.op_state {
            OperationState::Init => {

            }
            OperationState::Progress => {

            }
            OperationState::Done => {

            }
        }

        match self.interval.poll_next_unpin(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => {
                self.engine.write().finish_op(&self.operation);
                Poll::Ready(())
            }
        }
    }
}


trait OperationImpl {
    fn poll_init(&mut self, engine: &EngineRef, operation: &OperationRef) -> Poll<()>,
    fn poll_progress(&mut self, engine: &EngineRef, operation: &OperationRef) -> Poll<()>,
    fn poll_done(&mut self, engine: &EngineRef, operation: &OperationRef) -> Poll<()>,
}


fn main() -> EngineResult {
    let engine = EngineRef::new(Engine::new());

    let runtime = {
        let mut builder = runtime::Builder::new();
        builder.name_prefix("op-").build().unwrap()
    };

    {
        let mut e = engine.write();
        e.enqueue_op(OperationRef::new("ddd1"));
        e.enqueue_op(OperationRef::new("ddd2"));
        e.enqueue_op(OperationRef::new("ddd3"));
        e.enqueue_op(OperationRef::new("ddd4"));
        e.enqueue_op(OperationRef::new("ddd5"));
    }

    runtime.block_on(async {
        engine.main_task().await
    })
}
