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

struct Operation {
    id: Uuid,
    name: String,
    progress: f64,
}

impl Operation {
    fn new(name: &str) -> OperationRef {
        OperationRef::new(Operation {
            id: Uuid::new_v4(),
            name: name.into(),
            progress: 0.0,
        })
    }
}

type OperationRef = SyncRef<Operation>;

struct Engine {
    operation_queue1: VecDeque<OperationRef>,
    operation_queue2: VecDeque<OperationRef>,
    operations: LinkedHashMap<Uuid, OperationRef>,
    waker: Option<Waker>,
}

impl Engine {
    fn new() -> Engine {
        Engine {
            operation_queue1: VecDeque::new(),
            operation_queue2: VecDeque::new(),
            operations: LinkedHashMap::new(),
            waker: None,
        }
    }

    fn enqueue_op(&mut self, op: OperationRef) {
        self.operation_queue1.push_back(op);
        if let Some(ref w) = self.waker {
            w.wake_by_ref();
        }
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
        if self.engine.read().waker.is_none() {
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

        Poll::Ready(EngineResult { code: 0 })
    }
}

struct OperationTask {
    engine: EngineRef,
    operation: OperationRef,
    waker: Option<Waker>,
    interval: Interval,
}

impl OperationTask {
    fn new(engine: EngineRef, operation: OperationRef) -> OperationTask {
        OperationTask {
            engine,
            operation,
            waker: None,
            interval: Interval::new_interval(Duration::from_secs(3)),
        }
    }
}

impl Future for OperationTask {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.waker.is_none() {
            self.waker = Some(cx.waker().clone());
        }

        match self.interval.poll_next_unpin(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(_) => {
                self.engine.write().operations.remove(&self.operation.read().id);
                Poll::Ready(())
            }
        }
    }
}

fn main() -> EngineResult {
    let engine = EngineRef::new(Engine::new());

    let runtime = {
        let mut builder = runtime::Builder::new();
        builder.name_prefix("op-").build().unwrap()
    };

    engine.write().enqueue_op(Operation::new("ddd"));

    runtime.block_on(async move {
        engine.main_task().await
    })
}
