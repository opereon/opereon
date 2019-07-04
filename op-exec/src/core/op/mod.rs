use super::*;

use std::borrow::Cow;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use serde::{de, ser};
use uuid::Uuid;

mod context;
mod impls;
mod outcome;
mod progress;

pub use self::context::Context;
pub use self::impls::DiffMethod;
pub use self::outcome::Outcome;
pub use self::progress::{Progress, Unit};

use self::impls::{create_operation_impl, OperationImplType};

#[derive(Debug, Serialize, Deserialize)]
pub struct Operation {
    id: Uuid,
    label: String,
    context: Context,

    #[serde(skip)]
    progress: Progress,
    #[serde(skip)]
    progress_task: AtomicTask,
    #[serde(skip)]
    outcome: Option<Result<Outcome, RuntimeError>>,
    #[serde(skip)]
    outcome_task: AtomicTask,
    #[serde(skip)]
    task: AtomicTask,
    #[serde(skip)]
    blocked: bool,
    #[serde(skip)]
    cancelled: bool,
}

impl Operation {
    fn new(id: Uuid, label: Cow<str>, context: Context) -> Operation {
        Operation {
            id,
            label: label.into_owned(),
            context,
            progress: Progress::default(),
            progress_task: AtomicTask::new(),
            outcome: None,
            outcome_task: AtomicTask::new(),
            task: AtomicTask::new(),
            blocked: false,
            cancelled: false,
        }
    }

    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn context(&self) -> &Context {
        &self.context
    }

    pub(super) fn update_progress_value(&mut self, value: f64) {
        if self.progress.set_value(value) {
            self.progress_task.notify();
        }
    }

    pub(super) fn update_progress_value_done(&mut self) {
        if self.progress.set_value_done() {
            self.progress_task.notify();
        }
    }

    pub(super) fn update_progress_step_value(&mut self, step: usize, value: f64) {
        if self.progress.set_step_value(step, value) {
            self.progress_task.notify();
        }
    }

    pub(super) fn update_progress_step_value_done(&mut self, step: usize) {
        if self.progress.set_step_value_done(step) {
            self.progress_task.notify();
        }
    }

    pub fn is_blocked(&self) -> bool {
        self.blocked
    }

    pub(super) fn block(&mut self, block: bool) {
        self.blocked = block;
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
        self.outcome_task.notify();
    }
}

#[derive(Debug, Clone)]
pub struct OperationRef(Arc<RwLock<Operation>>);

impl OperationRef {
    pub fn new(id: Uuid, label: Cow<str>, context: Context) -> OperationRef {
        Self::wrap(Operation::new(id, label, context))
    }

    pub(super) fn wrap(operation: Operation) -> OperationRef {
        OperationRef(Arc::new(RwLock::new(operation)))
    }

    pub fn read(&self) -> RwLockReadGuard<Operation> {
        self.0.read().unwrap()
    }

    pub fn write(&self) -> RwLockWriteGuard<Operation> {
        self.0.write().unwrap()
    }

    pub(super) fn persist<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let data = rmp_serde::to_vec_named(self).unwrap();
        let mut fname = self.read().id().to_string();
        fname.push_str(".op");
        std::fs::write(path.as_ref().join(fname), &data)?;
        Ok(())
    }
}

impl ser::Serialize for OperationRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        self.read().serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for OperationRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let oper = Operation::deserialize(deserializer)?;
        Ok(OperationRef::wrap(oper))
    }
}

impl PartialEq for OperationRef {
    fn eq(&self, other: &OperationRef) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for OperationRef {}

unsafe impl Send for OperationRef {}

unsafe impl Sync for OperationRef {}

impl From<Context> for OperationRef {
    fn from(context: Context) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        OperationRef::new(
            Uuid::new_v4(),
            format!(
                "{}-{}",
                context.label(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ).into(),
            context,
        )
    }
}

#[derive(Debug)]
pub(super) struct OperationTask {
    operation: OperationRef,
    engine: EngineRef,
    inner: Box<OperationImplType>,
}

impl OperationTask {
    pub(super) fn new(
        operation: OperationRef,
        engine: EngineRef,
    ) -> Result<OperationTask, RuntimeError> {
        let impl_future = create_operation_impl(&operation, &engine)?;
        Ok(OperationTask {
            operation,
            engine,
            inner: impl_future,
        })
    }

    pub fn operation(&self) -> &OperationRef {
        &self.operation
    }

    pub fn engine(&self) -> &EngineRef {
        &self.engine
    }
}

impl Future for OperationTask {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        //println!("--- task poll {}", self.operation.read().label());

        if self.operation.read().is_cancelled() {
            self.inner.on_cancel().unwrap(); //FIXME (jc) handle panic and error
            self.operation.write().outcome = Some(Err(RuntimeError::Cancelled));
            self.engine.write().remove_operation(&self.operation);
            self.engine.read().notify();
        } else {
            self.operation.read().task.register();

            match self.inner.poll() {
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Ok(Async::Ready(outcome)) => {
                    let mut o = self.operation.write();
                    o.outcome = Some(Ok(outcome));
                    o.update_progress_value_done();
                }
                Err(err) => {
                    self.operation.write().outcome = Some(Err(err));
                }
            }

            self.operation.read().outcome_task.notify();

            self.engine.write().remove_operation(&self.operation);
            self.engine.read().notify();

        }
        Ok(Async::Ready(()))
    }
}

#[derive(Debug)]
pub struct OutcomeFuture {
    operation: OperationRef,
}

impl OutcomeFuture {
    pub(super) fn new(operation: OperationRef) -> OutcomeFuture {
        OutcomeFuture { operation }
    }

    pub fn into_exec(self) -> OperationExec {
        OperationExec::new(self)
    }
}

impl Future for OutcomeFuture {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        self.operation.read().outcome_task.register();

        match self.operation.write().outcome.take() {
            None => Ok(Async::NotReady),
            Some(Ok(outcome)) => Ok(Async::Ready(outcome)),
            Some(Err(err)) => Err(err),
        }
    }
}

#[derive(Debug)]
pub struct ProgressStream {
    operation: OperationRef,
    progress: Progress,
    done: bool,
}

impl ProgressStream {
    pub(super) fn new(operation: OperationRef) -> ProgressStream {
        let progress = operation.read().progress.clone();
        ProgressStream {
            operation,
            progress,
            done: false,
        }
    }
}

impl Stream for ProgressStream {
    type Item = Progress;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.done {
            Ok(Async::Ready(None))
        } else {
            self.operation.write().progress_task.register();

            let o = self.operation.read();
            let ref progress = o.progress;
            if self.progress.counter() != progress.counter() {
                if progress.is_done() {
                    self.done = true;
                }
                self.progress = progress.clone();
                Ok(Async::Ready(Some(progress.clone())))
            } else {
                Ok(Async::NotReady)
            }
        }
    }
}

#[derive(Debug)]
pub struct OperationExec {
    operation: OperationRef,
    outcome: OutcomeFuture,
    progress: ProgressStream,
}

impl OperationExec {
    fn new(outcome: OutcomeFuture) -> OperationExec {
        let operation = outcome.operation.clone();
        let progress = ProgressStream::new(operation.clone());
        OperationExec {
            operation,
            outcome,
            progress,
        }
    }

    pub fn operation(&self) -> &OperationRef {
        &self.operation
    }

    pub fn outcome(&self) -> &OutcomeFuture {
        &self.outcome
    }

    pub fn outcome_mut(&mut self) -> &mut OutcomeFuture {
        &mut self.outcome
    }

    pub fn progress(&self) -> &ProgressStream {
        &self.progress
    }

    pub fn progress_mut(&mut self) -> &mut ProgressStream {
        &mut self.progress
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operation_serialize_to_json() {
        let o = OperationRef::new(
            Uuid::nil(),
            "Main operation".into(),
            Context::ModelCommit(PathBuf::from("/home/opereon/model")),
        );
        let s = serde_json::to_string(&o).unwrap();

        let json = r#"
        {
          "id": "00000000-0000-0000-0000-000000000000",
          "label": "Main operation",
          "context": {
            "type": "model-store",
            "arg": "/home/opereon/model"
          }
        }
        "#;

        assert!(json_eq!(s, json));
    }

    #[test]
    fn operation_deserialize_from_json() {
        let json = r#"
        {
          "id": "00000000-0000-0000-0000-000000000000",
          "label": "Main operation",
          "context": {
            "type": "model-store",
            "arg": "/home/opereon/model"
          }
        }
        "#;

        let op: OperationRef = serde_json::from_str(json).unwrap();

        let o = op.read();
        assert_eq!(o.id(), Uuid::nil());
        assert_eq!(o.label(), "Main operation");
        assert_eq!(
            o.context(),
            &Context::ModelCommit(PathBuf::from("/home/opereon/model"))
        );
    }
}
