use crate::exec::file::rsync::CompareResult;

use super::*;

pub use self::config::FileConfig;
pub use self::rsync::FileCopyOperation;
pub use self::rsync::RsyncConfig;
use self::rsync::RsyncError;
use self::rsync::RsyncExecutor;

mod config;
mod rsync;

//FIXME (jc)
#[derive(Debug, Clone)]
pub enum FileError {
    Undef,
}

impl From<SshError> for FileError {
    fn from(err: SshError) -> Self {
        eprintln!("ssh error: {:?}", err);
        FileError::Undef
    }
}

impl From<std::io::Error> for FileError {
    fn from(err: std::io::Error) -> Self {
        eprintln!("io error: {:?}", err);
        FileError::Undef
    }
}

impl From<RsyncError> for FileError {
    fn from(err: RsyncError) -> Self {
        eprintln!("rsync error: {:?}", err);
        FileError::Undef
    }
}

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
    ) -> Result<CompareResult, FileError>;

    fn file_copy(
        &mut self,
        engine: &EngineRef,
        curr_dir: &Path,
        src_path: &Path,
        dst_path: &Path,
        chown: Option<&str>,
        chmod: Option<&str>,
        log: &OutputLog,
    ) -> Result<TaskResult, FileError>;
}

pub fn create_file_executor(
    host: &Host,
    engine: &EngineRef,
) -> CommandResult<Box<dyn FileExecutor>> {
    Ok(Box::new(RsyncExecutor::new(host, engine)))
}
