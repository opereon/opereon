use super::*;

use self::config::*;
use self::exec::*;
pub use self::model::DiffMethod;
use self::model::*;
use self::parallel::*;
use self::sequence::*;

mod config;
mod exec;
mod model;
mod parallel;
mod sequence;

pub trait OperationImpl:
    Future<Item = Outcome, Error = RuntimeError> + Send + Sync + Debug
{
    fn init(&mut self) -> RuntimeResult<()> {
        Ok(())
    }

    fn on_cancel(&mut self) -> RuntimeResult<()> {
        Ok(())
    }
}

pub type OperationImplType = dyn OperationImpl<Item = Outcome, Error = RuntimeError>;

pub fn create_operation_impl(
    operation: &OperationRef,
    engine: &EngineRef,
) -> RuntimeResult<Box<OperationImplType>> {
    let mut op_impl: Box<OperationImplType> = match *operation.read().context() {
        Context::ConfigGet => Box::new(ConfigGetOperation::new(operation.clone(), engine.clone())),
        Context::ModelCommit(ref path) => Box::new(ModelCommitOperation::new(
            operation.clone(),
            engine.clone(),
            path,
        )),
        Context::ModelQuery {
            ref model,
            ref expr,
        } => Box::new(ModelQueryOperation::new(
            operation.clone(),
            engine.clone(),
            model.clone(),
            expr.clone(),
        )),
        Context::ModelTest { ref model } => Box::new(ModelTestOperation::new(
            operation.clone(),
            engine.clone(),
            model.clone(),
        )),
        Context::ModelDiff {
            ref prev_model,
            ref next_model,
            method,
        } => Box::new(ModelDiffOperation::new(
            operation.clone(),
            engine.clone(),
            prev_model.clone(),
            next_model.clone(),
            method,
        )),
        Context::ModelUpdate {
            ref prev_model,
            ref next_model,
            dry_run,
        } => Box::new(ModelUpdateOperation::new(
            operation.clone(),
            engine.clone(),
            prev_model.clone(),
            next_model.clone(),
            dry_run,
        )),
        Context::ModelCheck {
            ref model,
            ref filter,
            dry_run,
        } => Box::new(ModelCheckOperation::new(
            operation.clone(),
            engine.clone(),
            model.clone(),
            filter.clone(),
            dry_run,
        )),
        Context::ModelProbe {
            ref ssh_dest,
            ref model,
            ref filter,
            ref args,
        } => Box::new(ModelProbeOperation::new(
            operation.clone(),
            engine.clone(),
            ssh_dest.clone(),
            model.clone(),
            filter.clone(),
            args,
        )),
        Context::ProcExec { ref exec_path } => Box::new(ProcExecOperation::new(
            operation.clone(),
            engine.clone(),
            exec_path,
        )?),
        Context::StepExec {
            ref exec_path,
            step_index,
        } => Box::new(StepExecOperation::new(
            operation.clone(),
            engine.clone(),
            exec_path,
            step_index,
        )?),
        Context::TaskExec {
            ref exec_path,
            step_index,
            task_index,
        } => Box::new(TaskExecOperation::new(
            operation.clone(),
            engine.clone(),
            exec_path,
            step_index,
            task_index,
        )?),
        Context::Sequence(ref steps) => Box::new(SequenceOperation::new(
            operation.clone(),
            engine.clone(),
            steps.clone(),
        )?),
        Context::Parallel(ref steps) => Box::new(ParallelOperation::new(
            operation.clone(),
            engine.clone(),
            steps.clone(),
        )?),
        Context::ModelInit { ref path } => Box::new(ModelInitOperation::new(
            operation.clone(),
            engine.clone(),
            path.clone(),
        )),
        Context::FileCopyExec {
            ref curr_dir,
            ref src_path,
            ref dst_path,
            ref chown,
            ref chmod,
            ref host,
        } => Box::new(FileCopyOperation::new(
            operation.clone(),
            engine.clone(),
            curr_dir,
            src_path,
            dst_path,
            chown,
            chmod,
            host,
        )),
        Context::RemoteExec {
            ref expr,
            ref command,
            ref model_path,
        } => Box::new(RemoteCommandOperation::new(
            operation.clone(),
            engine.clone(),
            expr.clone(),
            command.clone(),
            model_path.clone(),
        )),
    };

    op_impl.init().map_err(|err| {
        // stop progress future when initialization error occurs
        operation.write().update_progress_value_done();
        err
    })?;

    Ok(op_impl)
}
