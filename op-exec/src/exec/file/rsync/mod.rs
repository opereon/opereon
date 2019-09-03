use std::path::PathBuf;
use std::process::{Command, ExitStatus};

use os_pipe::PipeWriter;
use tokio::prelude::{Async, Future, Poll};

use crate::core::OperationImpl;
use crate::exec::file::rsync::compare::DiffInfo;
use crate::Host;

use super::*;

pub use self::config::RsyncConfig;
pub use self::copy::FileCopyOperation;

mod compare;
mod config;
mod copy;

type FileSize = u64;

pub type RsyncError = BasicDiag;
pub type RsyncResult<T> = Result<T, RsyncError>;

#[derive(Debug, Display, Detail)]
pub enum RsyncErrorDetail {
    #[display(fmt = "cannot spawn rsync process")]
    RsyncSpawn,

    #[display(fmt = "rsync process didn't exited successfully: {stderr}")]
    RsyncProcess { stderr: String },

    #[display(fmt = "rsync process didn't exited successfully: {status}")]
    RsyncProcessStatus { status: ExitStatus },

    #[display(fmt = "rsync process terminated")]
    RsyncTerminated,
}

impl RsyncErrorDetail {
    pub fn process_exit<T>(stderr: String) -> RsyncResult<T> {
        Err(RsyncErrorDetail::RsyncProcess { stderr }.into())
    }

    pub fn spawn_err(err: std::io::Error) -> RsyncError {
        let err = IoErrorDetail::from(err);
        RsyncErrorDetail::RsyncSpawn.with_cause(BasicDiag::from(err))
    }
}

pub type RsyncParseError = BasicDiag;
pub type RsyncParseResult<T> = Result<T, RsyncParseError>;

#[derive(Debug, Display, Detail)]
pub enum RsyncParseErrorDetail {
    #[display(fmt = "cannot parse rsync output - this error is probably \
                     caused by incompatible rsync binary version.\
                     If you think you have correct rsync version installed, please contact support.\
                     Error occurred in '{line}'.\n{output}")]
    Custom { line: u32, output: String },
}

impl RsyncParseErrorDetail {
    pub fn custom_line<T>(line: u32) -> RsyncParseResult<T> {
        Err(RsyncParseErrorDetail::Custom {
            line,
            output: String::new(),
        })
        .into_diag_res()
    }
    pub fn custom_output<T>(line: u32, output: String) -> RsyncParseResult<T> {
        Err(RsyncParseErrorDetail::Custom { line, output }).into_diag_res()
    }
}

#[derive(Debug, Clone)]
pub struct RsyncParams {
    current_dir: PathBuf,
    src_username: Option<String>,
    src_hostname: Option<String>,
    src_paths: Vec<PathBuf>,
    dst_username: Option<String>,
    dst_hostname: Option<String>,
    dst_path: PathBuf,
    chmod: Option<String>,
    chown: Option<String>,
    remote_shell: Option<String>,
}

#[allow(dead_code)]
impl RsyncParams {
    pub fn new<P1: Into<PathBuf>, P2: Into<PathBuf>, P3: Into<PathBuf>>(
        current_dir: P1,
        src_path: P2,
        dst_path: P3,
    ) -> RsyncParams {
        RsyncParams {
            current_dir: current_dir.into(),
            src_username: None,
            src_hostname: None,
            src_paths: vec![src_path.into()],
            dst_username: None,
            dst_hostname: None,
            dst_path: dst_path.into(),
            chmod: None,
            chown: None,
            remote_shell: None,
        }
    }

    pub fn src_username<S: Into<String>>(&mut self, username: S) -> &mut RsyncParams {
        self.src_username = Some(username.into());
        self
    }

    pub fn src_hostname<S: Into<String>>(&mut self, hostname: S) -> &mut RsyncParams {
        self.src_hostname = Some(hostname.into());
        self
    }

    pub fn add_src_path(&mut self, src_path: &PathBuf) -> &mut RsyncParams {
        self.src_paths.push(src_path.clone());
        self
    }

    pub fn dst_username<S: Into<String>>(&mut self, username: S) -> &mut RsyncParams {
        self.dst_username = Some(username.into());
        self
    }

    pub fn dst_hostname<S: Into<String>>(&mut self, hostname: S) -> &mut RsyncParams {
        self.dst_hostname = Some(hostname.into());
        self
    }

    pub fn chmod<S: Into<String>>(&mut self, chmod: S) -> &mut RsyncParams {
        self.chmod = Some(chmod.into());
        self
    }

    /// This option have effect only when the command is run as superuser.
    /// Available with rsync binary 3.1.0 and above.
    pub fn chown<S: Into<String>>(&mut self, chown: S) -> &mut RsyncParams {
        self.chown = Some(chown.into());
        self
    }

    pub fn remote_shell<S: Into<String>>(&mut self, shell: S) -> &mut RsyncParams {
        self.remote_shell = Some(shell.into());
        self
    }

    fn to_cmd(&self, config: &RsyncConfig) -> Command {
        fn print_host(hostname: Option<&String>, username: Option<&String>, out: &mut String) {
            use std::fmt::Write;

            match (hostname, username) {
                (Some(hostname), Some(username)) => write!(
                    out,
                    "{username}@{hostname}:",
                    username = username,
                    hostname = hostname
                )
                .unwrap(),
                (Some(hostname), None) => write!(out, "{hostname}:", hostname = hostname).unwrap(),
                _ => {}
            }
        }

        let mut cmd = Command::new(config.rsync_cmd());
        cmd.current_dir(&self.current_dir);

        let mut path = String::with_capacity(1024);

        for src_path in self.src_paths.iter() {
            path.clear();
            print_host(
                self.src_hostname.as_ref(),
                self.src_username.as_ref(),
                &mut path,
            );
            path.push_str(src_path.to_str().unwrap()); //FIXME (jc) handle non-utf8 output (should not be possible at the moment)
            cmd.arg(&path);
        }

        path.clear();
        print_host(
            self.dst_hostname.as_ref(),
            self.dst_username.as_ref(),
            &mut path,
        );
        path.push_str(self.dst_path.to_str().unwrap()); //FIXME (jc) handle non-utf8 output (should not be possible at the moment)
        cmd.arg(&path);

        if let Some(ref chmod) = self.chmod {
            cmd.arg("--chmod").arg(chmod);
        } else {
            cmd.arg("--perms"); // by default preserve permissions
        }

        cmd.arg("--group").arg("--owner"); // by default preserve group and owner, required by --chown

        if let Some(ref chown) = self.chown {
            cmd.arg("--chown").arg(chown);
        }

        if let Some(ref shell) = self.remote_shell {
            cmd.arg("-e").arg(shell);
        }

        cmd
    }
}

#[derive(Debug)]
pub struct RsyncExecutor {
    config: ConfigRef,
    host: Host,
}

impl RsyncExecutor {
    pub fn new(host: &Host, engine: &EngineRef) -> RsyncExecutor {
        RsyncExecutor {
            config: engine.read().config().clone(),
            host: host.clone(),
        }
    }

    fn config(&self) -> &RsyncConfig {
        self.config.exec().file().rsync()
    }
}

#[derive(Debug)]
pub struct CompareResult {
    diffs: Vec<DiffInfo>,
    status: Option<i32>,
    signal: Option<i32>,
}

impl CompareResult {
    pub fn new(diffs: Vec<DiffInfo>, status: Option<i32>, signal: Option<i32>) -> Self {
        Self {
            diffs,
            status,
            signal,
        }
    }
    pub fn is_success(&self) -> bool {
        if let Some(status) = self.status {
            status == 0
        } else {
            false
        }
    }

    pub fn diffs(&self) -> &Vec<DiffInfo> {
        &self.diffs
    }

    pub fn status(&self) -> Option<i32> {
        self.status
    }

    pub fn signal(&self) -> Option<i32> {
        self.signal
    }

    pub fn into_task_result(self) -> TaskResult {
        TaskResult::new(Outcome::Empty, self.status, self.signal)
    }
}

impl FileExecutor for RsyncExecutor {
    fn file_compare(
        &mut self,
        engine: &EngineRef,
        curr_dir: &Path,
        src_path: &Path,
        dst_path: &Path,
        chown: Option<&str>,
        chmod: Option<&str>,
        checksum: bool,
    ) -> FileResult<CompareResult> {
        let ssh_session = engine
            .write()
            .ssh_session_cache_mut()
            .get(self.host.ssh_dest())?;
        let mut params = RsyncParams::new(curr_dir, src_path, dst_path);
        params
            .dst_hostname(self.host.ssh_dest().hostname())
            .remote_shell(ssh_session.read().remote_shell_call());
        if let Some(chown) = chown {
            params.chown(chown);
        }
        if let Some(chmod) = chmod {
            params.chmod(chmod);
        }

        let diffs = self::rsync::compare::rsync_compare(self.config(), &params, checksum)?;
        let mut result = 0;
        // FIXME ws to be removed?
        for diff in &diffs {
            if diff.state().is_modified_chown() {
                println!("{}: incorrect owner/group", diff.file_path().display());
                result = std::cmp::max(result, 1);
            }
            if diff.state().is_modified_chmod() {
                println!("{}: incorrect permissions", diff.file_path().display());
                result = std::cmp::max(result, 1);
            }
            if diff.state().is_modified_content() {
                println!("{}: content differs", diff.file_path().display());
                result = std::cmp::max(result, 2);
            }
        }

        Ok(CompareResult::new(diffs, Some(result), None))
    }

    fn file_copy(
        &mut self,
        engine: &EngineRef,
        curr_dir: &Path,
        src_path: &Path,
        dst_path: &Path,
        chown: Option<&str>,
        chmod: Option<&str>,
        log: &OutputLog,
    ) -> FileResult<TaskResult> {
        let ssh_session = engine
            .write()
            .ssh_session_cache_mut()
            .get(self.host.ssh_dest())?;
        let mut params = RsyncParams::new(curr_dir, src_path, dst_path);
        params
            .dst_hostname(self.host.ssh_dest().hostname())
            .remote_shell(ssh_session.read().remote_shell_call());
        if let Some(chown) = chown {
            params.chown(chown);
        }
        if let Some(chmod) = chmod {
            params.chmod(chmod);
        }

        self::rsync::copy::rsync_copy(self.config(), &params, log)
    }
}
