use super::*;


#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(tag = "type", content = "arg")]
pub enum Context {
    ConfigGet,
    ModelList,
    ModelStore(PathBuf),
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
        model: ModelPath,
        name: String,
        args: Vec<(String, String)>,
    },
    ProcExec {
        bin_id: Uuid,
        exec_path: PathBuf,
    },
    StepExec {
        bin_id: Uuid,
        exec_path: PathBuf,
        step_index: usize,
    },
    TaskExec {
        bin_id: Uuid,
        exec_path: PathBuf,
        step_index: usize,
        task_index: usize,
    },
    Sequence(Vec<OperationRef>),
    Parallel(Vec<OperationRef>),
}

impl Context {
    pub fn label(&self) -> &str {
        match *self {
            Context::ConfigGet => "config-get",
            Context::ModelList => "model-list",
            Context::ModelStore(..) => "model-store",
            Context::ModelQuery {..} => "model-query",
            Context::ModelTest {..} => "model-test",
            Context::ModelDiff {..} => "model-diff",
            Context::ModelUpdate {..} => "model-update",
            Context::ModelCheck {..} => "model-check",
            Context::ModelProbe {..} => "model-probe",
            Context::ProcExec {..} => "proc-exec",
            Context::StepExec {..} => "step-exec",
            Context::TaskExec {..} => "task-exec",
            Context::Sequence(..) => "sequence",
            Context::Parallel(..) => "parallel",
        }
    }
}
