use crate::progress::Progress;
use crate::{EngineRef, OperationImpl};
use kg_utils::sync::SyncRef;
use std::ops::Deref;
use std::task::Waker;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum OperationState {
    Init,
    Progress,
    Done,
    Cancel,
}

pub struct Operation {
    id: Uuid,
    parent: Uuid,
    operations: Vec<Uuid>,
    name: String,
    progress: Progress,
    waker: Option<Waker>,
    op_state: OperationState,
    op_impl: Option<Box<dyn OperationImpl>>,
}

impl Operation {
    fn new<S: Into<String>, O: OperationImpl + 'static>(name: S, op_impl: O) -> Operation {
        Operation {
            id: Uuid::new_v4(),
            parent: Uuid::nil(),
            operations: Vec::new(),
            name: name.into(),
            progress: Progress::default(),
            waker: None,
            op_state: OperationState::Init,
            op_impl: Some(Box::new(op_impl)),
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
}

#[derive(Clone)]
pub struct OperationRef(SyncRef<Operation>);

impl Deref for OperationRef {
    type Target = SyncRef<Operation>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl OperationRef {
    pub fn new<S: Into<String>, O: OperationImpl + 'static>(name: S, op_impl: O) -> OperationRef {
        OperationRef(SyncRef::new(Operation::new(name, op_impl)))
    }

    pub fn id(&self) -> Uuid {
        self.0.read().id
    }

    pub fn set_waker(&self, waker: Waker) {
        self.write().set_waker(waker);
    }

    pub(crate) fn take_op_impl(&mut self) -> Option<Box<dyn OperationImpl>> {
        self.0.write().op_impl.take()
    }
}
