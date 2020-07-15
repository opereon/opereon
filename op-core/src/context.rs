use crate::ops::config::ConfigGetOperation;
use crate::ops::model::{ModelQueryOperation, ModelTestOperation, ModelDiffOperation, ModelCommitOperation, ModelInitOperation};
use crate::outcome::Outcome;
use op_engine::operation::OperationImplExt;
use op_engine::{OperationImpl, OperationRef};
use op_exec2::command::ssh::SshDest;
use op_rev::RevPath;
use std::path::PathBuf;

#[derive(PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type", content = "arg")]
pub enum Context {
    ConfigGet,
    ModelInit {
        path: PathBuf,
    },
    ModelCommit(String),
    ModelQuery {
        model: RevPath,
        expr: String,
    },
    ModelTest {
        model: RevPath,
    },
    ModelDiff {
        prev_model: RevPath,
        next_model: RevPath,
    },
    ModelUpdate {
        prev_model: RevPath,
        next_model: RevPath,
        dry_run: bool,
    },
    ModelCheck {
        model: RevPath,
        filter: Option<String>,
        dry_run: bool,
    },
    ModelProbe {
        ssh_dest: SshDest,
        model: RevPath,
        filter: Option<String>,
        args: Vec<(String, String)>,
    },
    ProcExec {
        exec_path: PathBuf,
    },
    StepExec {
        exec_path: PathBuf,
        step_index: usize,
    },
    TaskExec {
        exec_path: PathBuf,
        step_index: usize,
        task_index: usize,
    },
    FileCopyExec {
        curr_dir: PathBuf,
        src_path: PathBuf,
        dst_path: PathBuf,
        chown: Option<String>,
        chmod: Option<String>,
        // host: Host,
    },
    RemoteExec {
        expr: String,
        command: String,
        model_path: RevPath,
    },
    // Sequence(Vec<OperationRef<Outcome>>),
    // Parallel(Vec<OperationRef<Outcome>>),
}

impl Context {
    pub fn label(&self) -> &str {
        match *self {
            Context::ConfigGet => "config-get",
            Context::ModelInit { .. } => "model-init",
            Context::ModelCommit(..) => "model-store",
            Context::ModelQuery { .. } => "model-query",
            Context::ModelTest { .. } => "model-test",
            Context::ModelDiff { .. } => "model-diff",
            Context::ModelUpdate { .. } => "model-update",
            Context::ModelCheck { .. } => "model-check",
            Context::ModelProbe { .. } => "model-probe",
            Context::ProcExec { .. } => "proc-exec",
            Context::StepExec { .. } => "step-exec",
            Context::TaskExec { .. } => "task-exec",
            Context::FileCopyExec { .. } => "file-copy-exec",
            Context::RemoteExec { .. } => "remote-exec",
            // Context::Sequence(..) => "sequence",
            // Context::Parallel(..) => "parallel",
        }
    }
}

impl Into<OperationRef<Outcome>> for Context {
    fn into(self) -> OperationRef<Outcome> {
        let label = self.label().to_string();
        let op_impl = match self {
            Context::ModelInit {path} => ModelInitOperation::new(path).boxed(),
            Context::ConfigGet => ConfigGetOperation::new().boxed(),
            Context::ModelCommit(message) => ModelCommitOperation::new(message).boxed(),
            Context::ModelQuery { model, expr } => ModelQueryOperation::new(model, expr).boxed(),
            Context::ModelTest { model } => ModelTestOperation::new(model).boxed(),
            Context::ModelDiff { prev_model, next_model } => ModelDiffOperation::new(prev_model, next_model).boxed(),
            Context::ModelUpdate { prev_model, next_model, dry_run } => { unimplemented!()}
            Context::ModelCheck { model, filter, dry_run } => { unimplemented!()}
            Context::ModelProbe { ssh_dest, model, filter, args } => { unimplemented!()}
            Context::ProcExec { exec_path } => { unimplemented!()}
            Context::StepExec { exec_path, step_index } => { unimplemented!()}
            Context::TaskExec { exec_path, step_index, task_index } => { unimplemented!()}
            Context::FileCopyExec { curr_dir, src_path, dst_path, chown, chmod } => { unimplemented!()}
            Context::RemoteExec { expr, command, model_path } => { unimplemented!()}
        };
        OperationRef::new(label, op_impl)
    }
}
