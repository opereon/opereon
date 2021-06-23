use crate::{OperationImpl, OperationRef};
use kg_utils::collections::LinkedHashMap;
use kg_utils::sync::{SyncRef, SyncRefMapReadGuard, SyncRefReadGuard};
use std::collections::{HashMap, VecDeque};

use crate::operation::OperationResult;
use futures::lock::{Mutex, MutexGuard};
use kg_diag::Detail;
//use serde::export::{PhantomData, Formatter};
use std::any::{Any, TypeId};
use std::future::Future;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use uuid::Uuid;
use std::fmt::Debug;
use std::marker::PhantomData;

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
    stopped: bool,
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

pub type Service = Box<dyn Any + Send + 'static>;
pub type State = Box<dyn Any + Send + Sync + 'static>;

#[derive(Clone)]
pub struct EngineRef<T: Clone + 'static> {
    operations: SyncRef<Operations<T>>,
    core: SyncRef<Core<T>>,
    services: Arc<HashMap<TypeId, Arc<Mutex<Service>>>>,
    state: Arc<State>,
}

impl<T: Clone + 'static> EngineRef<T> {
    pub fn default() -> EngineRef<T> {
        EngineRef::new(vec![], ())
    }

    pub fn new<S: Any + Send + Sync + 'static>(
        services: Vec<Box<dyn Any + Send + 'static>>,
        state: S,
    ) -> EngineRef<T> {
        let services = services
            .into_iter()
            .map(|s| {
                // use as_ref() to get type of boxed struct instead of Box
                let type_id = s.as_ref().type_id();
                (type_id, Arc::new(Mutex::new(s)))
            })
            .collect::<HashMap<_, _>>();

        EngineRef {
            operations: SyncRef::new(Operations::new()),
            core: SyncRef::new(Core::new()),
            services: Arc::new(services),
            state: Arc::new(Box::new(state)),
        }
    }

    pub fn build_runtime() -> Runtime {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("engine")
            .build()
            .unwrap();
        runtime
    }

    pub async fn start(&self) -> EngineResult {
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

    pub fn enqueue_operation(&self, operation: OperationRef<T>) -> oneshot::Receiver<()> {
        let (done_tx, done_rx) = oneshot::channel();
        operation.write().set_done_sender(done_tx);

        self.operations.write().add_operation(operation);
        self.core.write().wake();
        done_rx
    }

    pub fn enqueue_with_res(
        &self,
        operation: OperationRef<T>,
    ) -> impl Future<Output=OperationResult<T>> {
        let receiver = self.enqueue_operation(operation.clone());

        async move {
            match receiver.await {
                Ok(_) => {
                    // outcome is always set after operation completion
                    operation.write().take_outcome().unwrap()
                }
                Err(_) => {
                    // should never happen since only way to drop sender is:
                    // - complete operation (notification sent before drop)
                    // - drop entire operation (engine will never drop operation without completion)
                    unreachable!()
                }
            }
        }
    }

    fn finish_operation(&self, operation: &OperationRef<T>, res: OperationResult<T>) {
        operation.write().set_outcome(res);
        // this is safe since operations scheduled with `enqueue_operation` always have `done_sender`
        let sender = operation.write().take_done_sender().unwrap();
        match sender.send(()) {
            Ok(_) => {}
            Err(_) => {
                // nothing to do here. Its totally ok to have receiver deallocated (Fire and forget scenario)
            }
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

    pub async fn service<S: 'static>(&self) -> Option<EngineServiceGuard<'_, S>> {
        let s = self.services.get(&TypeId::of::<S>());
        if let Some(service) = s {
            let guard = service.lock().await;
            Some(EngineServiceGuard {
                phantom: PhantomData::<S>,
                guard,
            })
        } else {
            None
        }
    }

    pub fn state<S: 'static>(&self) -> Option<&S> {
        self.state.downcast_ref::<S>()
    }
}

pub struct EngineServiceGuard<'a, S> {
    phantom: PhantomData<S>,
    guard: MutexGuard<'a, Box<dyn Any + Send + 'static>>,
}

impl<S: 'static> Deref for EngineServiceGuard<'_, S> {
    type Target = S;

    fn deref(&self) -> &S {
        // this is safe since only way to create this guard is through engine.service method.
        self.guard.downcast_ref().expect("Unexpected service type")
    }
}

impl<S: 'static> DerefMut for EngineServiceGuard<'_, S> {
    fn deref_mut(&mut self) -> &mut S {
        self.guard.downcast_mut().expect("Unexpected service type")
    }
}

pub struct EngineResult {
    code: i32,
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
    let inner = async move || {
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

impl<T: Debug + Clone + 'static> Debug for EngineRef<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("EngineRef")
            // .field(&self.some_field)

            .finish()
    }
}
