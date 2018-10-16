use super::*;

use std::sync::{Arc, Mutex, MutexGuard};
use std::cell::RefCell;


#[derive(Debug, Serialize, Deserialize)]
pub struct ProcExec {
    created: DateTime<Utc>,
    name: String,
    curr_model: ModelPath,
    #[serde(skip_serializing_if = "Option::is_none")]
    prev_model: Option<ModelPath>,
    proc_path: Opath,
    args: Arguments,
    run: RunExec,
    #[serde(skip)]
    path: PathBuf,
}



