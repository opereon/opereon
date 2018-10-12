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
    ExecWork {
        bin_id: Uuid,
        work_path: PathBuf,
    },
    ExecJob {
        bin_id: Uuid,
        work_path: PathBuf,
        job_index: usize,
    },
    ExecAction {
        bin_id: Uuid,
        work_path: PathBuf,
        job_index: usize,
        action_index: usize,
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
            Context::ExecWork {..} => "exec-work",
            Context::ExecJob {..} => "exec-job",
            Context::ExecAction {..} => "exec-action",
            Context::Sequence(..) => "sequence",
            Context::Parallel(..) => "parallel",
        }
    }
}
