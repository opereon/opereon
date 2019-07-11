use super::*;

use self::config::*;
use self::exec::*;
use self::model::*;
pub use self::model::DiffMethod;
use self::parallel::*;
use self::sequence::*;

mod config;
mod model;
mod exec;
mod sequence;
mod parallel;

pub enum WakeUpStatus {
    /// Operation is ready - all children operations finished.
    Ready(Result<Outcome, RuntimeError>),

    /// Operation is not ready - there are still unfinished children operations.
    NotReady
}

pub trait OperationImpl: Send + Sync + Debug {
    fn on_cancel(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    /// Executes operation synchronously
    fn execute(&mut self) -> Result<Outcome, RuntimeError>;

    /// Called when children operation is finished.
    /// This method is only called when operation internally calls `engine.enqueue_nested_operation()`.
    fn wake_up(&mut self, finished_op: OperationRef) -> WakeUpStatus { unimplemented!() }
}

// FIXME ws do not return Result
pub fn create_operation_impl(operation: &OperationRef, engine: &EngineRef) -> Box<dyn OperationImpl> {
    println!("create operation impl = {}", operation.read().label());
    let ctx = operation.read().context().clone();
    let op_impl: Box<dyn OperationImpl> = match ctx {
        Context::ConfigGet => Box::new(ConfigGetOperation::new(operation.clone(), engine.clone())),
        Context::ModelCommit(ref path) => Box::new(ModelCommitOperation::new(operation.clone(), engine.clone(), path)),
        Context::ModelQuery { ref model, ref expr } => Box::new(ModelQueryOperation::new(operation.clone(), engine.clone(), model.clone(), expr.clone())),
        Context::ModelTest { ref model } => Box::new(ModelTestOperation::new(operation.clone(), engine.clone(), model.clone())),
        Context::ModelDiff { ref prev_model, ref next_model, method } => Box::new(ModelDiffOperation::new(operation.clone(), engine.clone(), prev_model.clone(), next_model.clone(), method)),
        Context::ModelUpdate { ref prev_model, ref next_model, dry_run } => Box::new(ModelUpdateOperation::new(operation.clone(), engine.clone(), prev_model.clone(), next_model.clone(), dry_run)),
        Context::ModelCheck { ref model, ref filter, dry_run } => Box::new(ModelCheckOperation::new(operation.clone(), engine.clone(), model.clone(), filter.clone(), dry_run)),
        Context::ModelProbe { ref ssh_dest, ref model, ref filter, ref args } => Box::new(ModelProbeOperation::new(operation.clone(), engine.clone(), ssh_dest.clone(), model.clone(), filter.clone(), args)),
        Context::ProcExec { bin_id, ref exec_path } => Box::new(ProcExecOperation::new(operation.clone(), engine.clone(), bin_id, exec_path)),
        Context::StepExec { bin_id, ref exec_path, step_index } => Box::new(StepExecOperation::new(operation.clone(), engine.clone(), bin_id, exec_path, step_index)),
        Context::TaskExec { bin_id, ref exec_path, step_index, task_index } => Box::new(TaskExecOperation::new(operation.clone(), engine.clone(), bin_id, exec_path, step_index, task_index)),
        Context::Sequence(ref steps) => Box::new(SequenceOperation::new(operation.clone(), engine.clone(), steps.clone())),
        Context::Parallel(ref steps) => Box::new(ParallelOperation::new(operation.clone(), engine.clone(), steps.clone())),
        Context::ModelInit => { Box::new(ModelInitOperation::new(operation.clone(), engine.clone()))}
        Context::FileCopyExec { bin_id, ref curr_dir, ref src_path, ref dst_path, ref chown, ref chmod, ref host} => Box::new(FileCopyOperation::new(operation.clone(), engine.clone(), bin_id, curr_dir, src_path, dst_path, chown, chmod, host)),
    };

    op_impl
}
