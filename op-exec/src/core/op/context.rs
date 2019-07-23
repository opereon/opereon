use super::*;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type", content = "arg")]
pub enum Context {
    ConfigGet,
    ModelInit,
    ModelCommit(String),
    ModelQuery {
        model: ModelPath,
        expr: String,
    },
    ModelTest {
        model: ModelPath,
    },
    ModelDiff {
        prev_model: ModelPath,
        next_model: ModelPath,
        method: DiffMethod,
    },
    ModelUpdate {
        prev_model: ModelPath,
        next_model: ModelPath,
        dry_run: bool,
    },
    ModelCheck {
        model: ModelPath,
        filter: Option<String>,
        dry_run: bool,
    },
    ModelProbe {
        ssh_dest: SshDest,
        model: ModelPath,
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
        model_path: ModelPath,
    },
    Sequence(Vec<OperationRef>),
    Parallel(Vec<OperationRef>),
}

impl Context {
    pub fn label(&self) -> &str {
        match *self {
            Context::ConfigGet => "config-get",
            Context::ModelInit => "model-init",
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
