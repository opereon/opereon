use super::*;

use std::sync::{Arc, Mutex, MutexGuard};
use std::cell::RefCell;

thread_local!(static EXEC_PATH: RefCell<PathBuf> = RefCell::new(PathBuf::new()));


#[derive(Debug, Serialize, Deserialize)]
pub struct ProcExec {
    created: DateTime<Utc>,
    name: String,
    model_path: ModelPath,
    proc_path: Opath,
    args: Arguments,
    #[serde(serialize_with = "ProcExec::store_runs", deserialize_with = "ProcExec::load_runs")]
    runs: Vec<RunExec>,
    #[serde(skip)]
    path: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct AsPath {
    path: String
}


