use super::*;


pub struct StepExec {
    host_path: Opath,
    step: usize,
    host: Host,
    tasks: Vec<TaskExec>,
    #[serde(skip)]
    path: PathBuf,
}
