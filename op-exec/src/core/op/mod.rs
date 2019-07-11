use std::borrow::Cow;
use std::fmt::Debug;
use std::ops::Deref;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::{de, ser};
use uuid::Uuid;

use super::*;

pub use self::context::Context;
pub use self::impls::{create_operation_impl};
pub use self::impls::DiffMethod;
pub use self::impls::{OperationImpl, WakeUpStatus};
pub use self::outcome::Outcome;
pub use self::progress::{Progress, Unit};
use std::path::Path;
use std::any::Any;
use std::sync::mpsc::SyncSender;

mod context;
mod impls;
mod outcome;
mod progress;

pub type OperationState = Arc<Mutex<Box<dyn Any>>>;

#[derive(Debug, Serialize, Deserialize)]
pub struct Operation {
    id: Uuid,
    label: String,
    context: Context,

    /// Internal operation state.
    /// Typically used by implementations between [`OperationImpl::wake_up`] calls.
    /// Allows to maintain `state machine`.
    ///
    /// [`OperationImpl::wake_up`]: trait.OperationImpl.html
    #[serde(skip)]
    state: Option<OperationState>,

    /// Result of this operation. Available when computation is finished.
    /// Utilized ONLY when the operation is scheduled from inside of another operation.
    #[serde(skip)]
    result: Option<Result<Outcome, RuntimeError>>,

    /// Flag indicating if operation is waiting for child operation completion.
    /// `true` only after [`EngineRef::enqueue_nested_operation`].
    ///
    /// [`EngineRef::enqueue_nested_operation`]: struct.EngineRef.html
    #[serde(skip)]
    waiting: bool,

    /// Operation result sender. Used to notify that operation is finished.
    #[serde(skip)]
    res_sender: Option<SyncSender<Result<Outcome, RuntimeError>>>,

    #[serde(skip)]
    cancelled: bool,
}

impl Operation {
    fn new(id: Uuid, label: Cow<str>, context: Context) -> Operation {
        Operation {
            id,
            label: label.into_owned(),
            context,
            state: None,
            result: None,
            waiting: false,
            res_sender: None,
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

    pub fn set_state(&mut self, state: OperationState) {
        self.state = Some(state)
    }

    pub fn state_mut(&mut self) -> Option<&mut OperationState> {
        self.state.as_mut()
    }

    pub fn set_result(&mut self, result: Result<Outcome, RuntimeError>) {
        self.result = Some(result)
    }

    pub fn take_result(&mut self) -> Option<Result<Outcome, RuntimeError>> {
        self.result.take()
    }

    pub fn set_waiting(&mut self, waiting: bool) {
        self.waiting = waiting
    }

    pub fn is_waiting(&self) -> bool {
        self.waiting
    }

    pub fn set_res_sender(&mut self, sender: SyncSender<Result<Outcome, RuntimeError>>) {
        self.res_sender = Some(sender);
    }

    pub fn res_sender_mut(&mut self) -> Option<&mut SyncSender<Result<Outcome, RuntimeError>>> {
        self.res_sender.as_mut()
    }
    pub fn take_res_sender(&mut self) -> SyncSender<Result<Outcome, RuntimeError>> {
        self.res_sender.take().unwrap()
    }

//    pub (crate) fn update_progress(&mut self, progress: Progress) {
//        self.progress = progress;
//        self.progress_task.notify();
//    }

//    pub(crate) fn update_progress_step(&mut self, step: usize, progress: Progress) {
//        self.progress.set_step(step, progress);
//        self.progress_task.notify();
//    }

//    pub(crate) fn update_progress_value(&mut self, value: f64) {
//        if self.progress.set_value(value) {
//            self.progress_task.notify();
//        }
//    }
//
//    pub(crate) fn update_progress_value_done(&mut self) {
//        if self.progress.set_value_done() {
//            self.progress_task.notify();
//        }
//    }
//
//    pub(crate) fn update_progress_step_value(&mut self, step: usize, value: f64) {
//        if self.progress.set_step_value(step, value) {
//            self.progress_task.notify();
//        }
//    }
//
//    pub(crate) fn update_progress_step_value_done(&mut self, step: usize) {
//        if self.progress.set_step_value_done(step) {
//            self.progress_task.notify();
//        }
//    }
//
//    pub(crate) fn set_progress(&mut self, progress: Progress) {
//        self.progress = progress;
//        self.progress_task.notify()
//    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub fn cancel(&mut self) {
        self.cancelled = true;
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
//        eprintln!("read operation = {:?}", self.0.read().unwrap().label());
        self.0.read().unwrap()
    }

    pub fn write(&self) -> RwLockWriteGuard<Operation> {
//        eprintln!("read operation = {:?}", self.0.read().unwrap().label());
        self.0.write().unwrap()
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

#[cfg(test)]
mod tests {
    use super::*;

//    #[test]
//    fn operation_serialize_to_json() {
//        let o = OperationRef::new(
//            Uuid::nil(),
//            "Main operation".into(),
//            Context::ModelCommit(String::from("/home/opereon/model")),
//        );
//        let s = serde_json::to_string(&o).unwrap();
//
//        let json = r#"
//        {
//          "id": "00000000-0000-0000-0000-000000000000",
//          "label": "Main operation",
//          "context": {
//            "type": "model-store",
//            "arg": "/home/opereon/model"
//          }
//        }
//        "#;
//
//        assert!(json_eq!(s, json));
//    }

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
            &Context::ModelCommit(String::from("/home/opereon/model"))
        );
    }
}
