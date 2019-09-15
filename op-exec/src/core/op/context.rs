use super::*;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
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
        host: Host,
    },
    RemoteExec {
        expr: String,
        command: String,
        model_path: RevPath,
    },
    Sequence(Vec<OperationRef>),
    Parallel(Vec<OperationRef>),
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
            Context::Sequence(..) => "sequence",
            Context::Parallel(..) => "parallel",
        }
    }
}
