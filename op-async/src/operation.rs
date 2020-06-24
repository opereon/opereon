use crate::progress::{Progress, ProgressUpdate};
use crate::EngineRef;
use kg_utils::sync::SyncRef;
use std::ops::Deref;
use std::task::Waker;
use uuid::Uuid;

use async_trait::async_trait;
use kg_diag::BasicDiag;
use kg_diag::Severity;

use tokio::sync::oneshot;
use tokio::sync::mpsc;

pub type OperationError = BasicDiag;
pub type OperationResult<T> = Result<T, OperationError>;
#[derive(Debug, Display)]
pub enum OperationErrorDetail {
    #[display(fmt = "operation cancelled by user")]
    Cancelled,
}

#[async_trait]
pub trait OperationImpl<T: Clone + 'static>: Send {
    async fn init(
        &mut self,
        _engine: &EngineRef<T>,
        _operation: &OperationRef<T>,
    ) -> OperationResult<()> {
        Ok::<_, OperationError>(())
    }

    async fn next_progress(
        &mut self,
        _engine: &EngineRef<T>,
        _operation: &OperationRef<T>,
    ) -> OperationResult<ProgressUpdate> {
        Ok::<_, OperationError>(ProgressUpdate::done())
    }

    async fn done(
        &mut self,
        _engine: &EngineRef<T>,
        _operation: &OperationRef<T>,
    ) -> OperationResult<T>;
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OperationState {
    Init,
    Progress,
    Done,
    Cancel,
}

pub struct Operation<T> {
    id: Uuid,
    parent: Uuid,
    operations: Vec<Uuid>,
    name: String,
    progress: Progress,
    waker: Option<Waker>,
    op_state: OperationState,
    op_impl: Option<Box<dyn OperationImpl<T>>>,
    outcome: Option<OperationResult<T>>,
    done_sender: Option<oneshot::Sender<()>>,
    cancel_sender: Option<mpsc::Sender<()>>,
    cancel_receiver: Option<mpsc::Receiver<()>>
}

impl<T: Clone + 'static> Operation<T> {
    fn new<S: Into<String>, O: OperationImpl<T> + 'static>(name: S, op_impl: O) -> Operation<T> {
        Operation {
            id: Uuid::new_v4(),
            parent: Uuid::nil(),
            operations: Vec::new(),
            name: name.into(),
            progress: Progress::default(),
            waker: None,
            op_state: OperationState::Init,
            op_impl: Some(Box::new(op_impl)),
            outcome: None,
            done_sender: None,
            cancel_sender: None,
            cancel_receiver: None
        }
    }

    pub fn wake(&mut self) {
        if let Some(w) = self.waker.take() {
            w.wake();
        }
    }

    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    pub fn parent(&self) -> Option<Uuid> {
        if self.parent.is_nil() {
            None
        } else {
            Some(self.parent)
        }
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    pub fn progress_mut(&mut self) -> &mut Progress {
        &mut self.progress
    }

    pub fn outcome(&self) -> Option<&OperationResult<T>> {
        self.outcome.as_ref()
    }
    pub fn take_outcome(&mut self) -> Option<OperationResult<T>> {
        self.outcome.take()
    }

    pub fn set_outcome(&mut self, outcome: OperationResult<T>) {
        self.outcome = Some(outcome)
    }

    pub (crate) fn set_done_sender(&mut self, sender: oneshot::Sender<()>) {
        self.done_sender = Some(sender)
    }

    pub (crate) fn take_done_sender(&mut self) -> Option<oneshot::Sender<()>>{
        self.done_sender.take()
    }

    pub (crate) fn set_cancel_sender(&mut self, sender: mpsc::Sender<()>) {
        self.cancel_sender = Some(sender)
    }

    pub (crate) fn cancel_sender(&self) -> Option<&mpsc::Sender<()>> {
        self.cancel_sender.as_ref()
    }

    pub (crate) fn set_cancel_receiver(&mut self, sender: mpsc::Receiver<()>) {
        self.cancel_receiver = Some(sender)
    }

    pub (crate) fn take_cancel_receiver(&mut self) -> Option<mpsc::Receiver<()>>{
        self.cancel_receiver.take()
    }
}

#[derive(Clone)]
pub struct OperationRef<T>(SyncRef<Operation<T>>);

impl<T> Deref for OperationRef<T> {
    type Target = SyncRef<Operation<T>>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Clone + 'static> OperationRef<T> {
    pub fn new<S: Into<String>, O: OperationImpl<T> + 'static>(
        name: S,
        op_impl: O,
    ) -> OperationRef<T> {
        OperationRef(SyncRef::new(Operation::new(name, op_impl)))
    }

    pub fn id(&self) -> Uuid {
        self.0.read().id
    }

    pub fn set_waker(&self, waker: Waker) {
        self.write().set_waker(waker);
    }

    pub(crate) fn take_op_impl(&mut self) -> Option<Box<dyn OperationImpl<T>>> {
        self.0.write().op_impl.take()
    }
}
