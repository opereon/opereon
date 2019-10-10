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

struct Operation {
    id: Uuid,
    name: String,
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

        Poll::Ready(EngineResult { code: 0 })
    }
}

fn main() -> EngineResult {
    let engine = EngineRef::new(Engine::new());

    let runtime = {
        let mut builder = runtime::Builder::new();
        builder.name_prefix("op-").build().unwrap()
    };

    engine.write().enqueue_op(OperationRef::new(Operation {
        id: Uuid::nil(),
        name: String::from("ddd"),
    }));

    runtime.block_on(async move {
        engine.main_task().await
    })
}
