use crate::exec::file::rsync::CompareResult;

use super::*;

pub use self::config::FileConfig;
pub use self::rsync::FileCopyOperation;
pub use self::rsync::RsyncConfig;
use self::rsync::RsyncExecutor;

mod config;
mod rsync;

pub type FileError = BasicDiag;
pub type FileResult<T> = Result<T, FileError>;

pub trait FileExecutor {
    fn file_compare(
        &mut self,
        engine: &EngineRef,
        curr_dir: &Path,
        src_path: &Path,
        dst_path: &Path,
        chown: Option<&str>,
        chmod: Option<&str>,
        checksum: bool,
        log: &OutputLog
    ) -> FileResult<CompareResult>;

    fn file_copy(
        &mut self,
        engine: &EngineRef,
        curr_dir: &Path,
        src_path: &Path,
        dst_path: &Path,
        chown: Option<&str>,
        chmod: Option<&str>,
        log: &OutputLog,
    ) -> FileResult<TaskResult>;
}

pub fn create_file_executor(
    host: &Host,
    engine: &EngineRef,
) -> CommandResult<Box<dyn FileExecutor>> {
    Ok(Box::new(RsyncExecutor::new(host, engine)))
}
