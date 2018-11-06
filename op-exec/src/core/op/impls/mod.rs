use super::*;

mod config;
mod model;
mod exec;
mod sequence;
mod parallel;

pub use self::model::DiffMethod;

use self::config::*;
use self::model::*;
use self::exec::*;
use self::sequence::*;
use self::parallel::*;


pub trait OperationImpl: Future<Item = Outcome, Error = RuntimeError> + Send + Sync + Debug {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn on_cancel(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}

pub type OperationImplType = OperationImpl<Item = Outcome, Error = RuntimeError>;

pub fn create_operation_impl(operation: &OperationRef, engine: &EngineRef) -> Result<Box<OperationImplType>, RuntimeError> {
    let mut op_impl: Box<OperationImplType> = match *operation.read().context() {
        Context::ConfigGet => Box::new(ConfigGetOperation::new(operation.clone(), engine.clone())),
        Context::ModelList => Box::new(ModelListOperation::new(operation.clone(), engine.clone())),
        Context::ModelStore(ref path) => Box::new(ModelStoreOperation::new(operation.clone(), engine.clone(), path.to_path_buf())),
        Context::ModelQuery { ref model, ref expr } => Box::new(ModelQueryOperation::new(operation.clone(), engine.clone(), model.clone(), expr.clone())),
        Context::ModelTest { ref model } => Box::new(ModelTestOperation::new(operation.clone(), engine.clone(), model.clone())),
        Context::ModelDiff { ref prev_model, ref next_model, method } => Box::new(ModelDiffOperation::new(operation.clone(), engine.clone(), prev_model.clone(), next_model.clone(), method)),
        Context::ModelUpdate { ref prev_model, ref next_model, dry_run } => Box::new(ModelUpdateOperation::new(operation.clone(), engine.clone(), prev_model.clone(), next_model.clone(), dry_run)),
        Context::ModelCheck { ref model, ref filter, dry_run } => Box::new(ModelCheckOperation::new(operation.clone(), engine.clone(), model.clone(), filter.clone(), dry_run)),
        Context::ModelProbe { ref ssh_dest, ref model, ref filter, ref args } => Box::new(ModelProbeOperation::new(operation.clone(), engine.clone(), model.clone(), filter.clone(), args)),
        Context::ProcExec { bin_id, ref exec_path } => Box::new(ProcExecOperation::new(operation.clone(), engine.clone(), bin_id, exec_path)?),
        Context::StepExec { bin_id, ref exec_path, step_index } => Box::new(StepExecOperation::new(operation.clone(), engine.clone(), bin_id, exec_path, step_index)?),
        Context::TaskExec { bin_id, ref exec_path, step_index, task_index } => Box::new(TaskExecOperation::new(operation.clone(), engine.clone(), bin_id, exec_path, step_index, task_index)?),
        Context::Sequence(ref steps) => Box::new(SequenceOperation::new(operation.clone(), engine.clone(), steps.clone())?),
        Context::Parallel(ref steps) => Box::new(ParallelOperation::new(operation.clone(), engine.clone(), steps.clone())?),
    };

    op_impl.init()?;

    Ok(op_impl)
}
