use super::*;

mod copy;
mod compare;
mod config;

pub use self::config::RsyncConfig;

use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use tokio::prelude::{Async, Future, Poll};
use crate::core::OperationImpl;
use crate::{Host, RuntimeError};
use std::thread::JoinHandle;
use os_pipe::PipeWriter;

pub type RsyncResult<T> = Result<T, RsyncError>;

#[derive(Debug)]
pub struct ParseError {
    line: u32
}

#[derive(Debug)]
pub enum RsyncError {
    IoError(std::io::Error),
    RsyncProcessTerminated,
    ParseError(ParseError),
    SshError(SshError),
}

impl From<ParseError> for RsyncError {
    fn from(err: ParseError) -> Self {
        RsyncError::ParseError(err)
    }
}

impl From<std::io::Error> for RsyncError {
    fn from(err: std::io::Error) -> Self {
        RsyncError::IoError(err)
    }
}

impl From<SshError> for RsyncError {
    fn from(err: SshError) -> Self {
        RsyncError::SshError(err)
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

impl RsyncParams {
    pub fn new<P1: Into<PathBuf>, P2: Into<PathBuf>, P3: Into<PathBuf>>(current_dir: P1, src_path: P2, dst_path: P3) -> RsyncParams {
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
                (Some(hostname), Some(username)) => write!(out, "{username}@{hostname}:", username = username, hostname = hostname).unwrap(),
                (Some(hostname), None) => write!(out, "{hostname}:", hostname = hostname).unwrap(),
                _ => {},
            }
        }

        let mut cmd = Command::new(config.rsync_cmd());
        cmd.current_dir(&self.current_dir);

        let mut path = String::with_capacity(1024);

        for src_path in self.src_paths.iter() {
            path.clear();
            print_host(self.src_hostname.as_ref(), self.src_username.as_ref(), &mut path);
            path.push_str(src_path.to_str().unwrap()); //FIXME (jc) handle non-utf8 output (should not be possible at the moment)
            cmd.arg(&path);
        }

        path.clear();
        print_host(self.dst_hostname.as_ref(), self.dst_username.as_ref(), &mut path);
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

impl FileExecutor for RsyncExecutor {
    fn file_compare(&mut self,
                    engine: &EngineRef,
                    curr_dir: &Path,
                    src_path: &Path,
                    dst_path: &Path,
                    chown: Option<&str>,
                    chmod: Option<&str>,
                    log: &OutputLog) -> Result<TaskResult, FileError> {
        let ssh_session = engine.write().ssh_session_cache_mut().get(self.host.ssh_dest())?;
        let mut params = RsyncParams::new(curr_dir, src_path, dst_path);
        params
            //.dst_hostname(self.host.ssh_dest().hostname())
            .remote_shell(ssh_session.read().remote_shell_call());
        if let Some(chown) = chown {
            params.chown(chown);
        }
        if let Some(chmod) = chmod {
            params.chmod(chmod);
        }

        let diffs = self::rsync::compare::rsync_compare(self.config(), &params)?;
        let mut result = 0;
        for diff in diffs {
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

        Ok(TaskResult::new(Outcome::Empty, Some(result), None))
    }

    fn file_copy(&mut self,
                 engine: &EngineRef,
                 curr_dir: &Path,
                 src_path: &Path,
                 dst_path: &Path,
                 chown: Option<&str>,
                 chmod: Option<&str>,
                 log: &OutputLog) -> Result<TaskResult, FileError> {
        let ssh_session = engine.write().ssh_session_cache_mut().get(self.host.ssh_dest())?;
        let mut params = RsyncParams::new(curr_dir, src_path, dst_path);
        params
            //.dst_hostname(self.host.ssh_dest().hostname())
            .remote_shell(ssh_session.read().remote_shell_call());
        if let Some(chown) = chown {
            params.chown(chown);
        }
        if let Some(chmod) = chmod {
            params.chmod(chmod);
        }

        self::rsync::copy::rsync_copy(self.config(), &params).map_err(|err| err.into())
    }
}
#[derive(Debug)]
pub struct FileCopyOperation {
    operation: OperationRef,
    engine: EngineRef,
    bin_id: Uuid,
    curr_dir: PathBuf,
    src_path: PathBuf,
    dst_path: PathBuf,
    chown: Option<String>,
    chmod: Option<String>,
    host: Host,
    status: Arc<Mutex<Option<Result<ExitStatus, RuntimeError>>>>,
    running: bool
}

impl FileCopyOperation {
    pub fn new(operation: OperationRef,
               engine: EngineRef,
               bin_id: Uuid,
               curr_dir: &Path,
               src_path: &Path,
               dst_path: &Path,
               chown: &Option<String>,
               chmod: &Option<String>,
               host: &Host) -> FileCopyOperation {
        FileCopyOperation {
            operation,
            engine,
            bin_id,
            curr_dir: curr_dir.to_owned(),
            src_path: src_path.to_owned(),
            dst_path: dst_path.to_owned(),
            chown: chown.as_ref().map(|s|s.to_string()),
            chmod: chmod.as_ref().map(|s|s.to_string()),
            host: host.clone(),
            status: Arc::new(Mutex::new(None)),
            running: false
        }
    }

    fn prepare_params(&self) -> Result<RsyncParams, CommandError>{
        let ssh_session = self.engine.write().ssh_session_cache_mut().get(self.host.ssh_dest())?;
        let mut params = RsyncParams::new(&self.curr_dir, &self.src_path, &self.dst_path);
        params
            //.dst_hostname(self.host.ssh_dest().hostname())
            .remote_shell(ssh_session.read().remote_shell_call());
        if let Some(chown) = &self.chown {
            params.chown(chown.to_owned());
        }
        if let Some(chmod) = &self.chmod {
            params.chmod(chmod.to_owned());
        }
        Ok(params)
    }

    fn spawn_std_watchers(&self) -> Result<(PipeWriter, PipeWriter), CommandError>{
        use std::io::BufRead;
        let (stdout, stdout_writer) = pipe()?;
        let (stderr, stderr_writer) = pipe()?;

        let run_stdout = move || {
            let buf = BufReader::new(stdout);

            for line in buf.lines() {
                match line {
                    Ok(line) => println!("out: {}", line),
                    Err(err) => return Err(err),
                }
            }
            Ok(())
        };

        let run_stderr = move || {
            let buf = BufReader::new(stderr);

            for line in buf.lines() {
                match line {
                    Ok(line) => println!("err: {}", line),
                    Err(err) => return Err(err),
                }
            }
            Ok(())
        };

        let hout: JoinHandle<std::io::Result<()>> = std::thread::spawn(run_stdout);
        let herr: JoinHandle<std::io::Result<()>> = std::thread::spawn(run_stderr);
        Ok((stdout_writer, stderr_writer))
    }

    pub fn status(&self) -> MutexGuard<Option<Result<ExitStatus, RuntimeError>>>{
        self.status.lock().unwrap()
    }
}

impl Future for FileCopyOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        println!("File copy!");

        if !self.running {
            let params = self.prepare_params()?;
            let config = self.engine.read().config().exec().file().rsync().clone();

            let (stdout, stderr) = self.spawn_std_watchers()?;

            let status = self.status.clone();
            let operation = self.operation.clone();

            std::thread::spawn(move || {
                let execute_cmd =  move || -> Result<ExitStatus, RuntimeError> {
                    let mut child = {
                        let mut rsync_cmd = params.to_cmd(&config);
                        rsync_cmd
                            .arg("--progress")
                            .arg("--super") // fail on permission denied
                            .arg("--recursive")
                            .arg("--links") // copy symlinks as symlinks
                            .arg("--times") // preserve modification times
                            .arg("--out-format=[%f][%l]")
                            .env("TERM", "xterm-256color")
                            .stdin(Stdio::null())
                            .stdout(Stdio::from(stdout))
                            .stderr(Stdio::from(stderr))
                            .spawn()?
                    };
                    Ok(child.wait()?)
                };

                match execute_cmd() {
                    Ok(stat) => {
                        *status.lock().unwrap() = Some(Ok(stat))
                    }
                    Err(err)=> {
                        *status.lock().unwrap() = Some(Err(err))
                    }
                }
                operation.write().notify()
            });
            self.running = true;
            return Ok(Async::NotReady)
        }

        if let Some(ref res) = *self.status() {
            match res {
                Ok(status) => {
                    if status.success() {
                        Ok(Async::Ready(Outcome::Empty))
                    } else {
                        Err(RuntimeError::Custom)
                    }
                }
                Err(err) => Err(RuntimeError::Custom)
            }
        } else {
            Ok(Async::NotReady)
        }
    }
}

impl OperationImpl for FileCopyOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}
