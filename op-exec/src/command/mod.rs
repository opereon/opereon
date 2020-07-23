use super::*;

use kg_diag::io::ResultExt;

use crate::utils::spawn_blocking;
use shared_child::SharedChild;
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::Stdio;
use std::process::{Command, ExitStatus};
use std::sync::Arc;
use tokio::sync::oneshot;

pub mod local;
pub mod ssh;
pub mod config;

pub type CommandError = BasicDiag;
pub type CommandResult<T> = Result<T, CommandError>;

#[derive(Debug, Display, Detail)]
pub enum CommandErrorDetail {
    #[display(fmt = "cannot spawn command")]
    CommandSpawn,

    #[display(fmt = "malformed command output")]
    MalformedOutput,
}

impl CommandErrorDetail {
    pub fn spawn_err(err: std::io::Error) -> CommandError {
        let err = IoErrorDetail::from(err);
        CommandErrorDetail::CommandSpawn.with_cause(BasicDiag::from(err))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandOutput {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl CommandOutput {
    pub fn new(code: Option<i32>, stdout: String, stderr: String) -> Self {
        CommandOutput {
            code,
            stdout,
            stderr,
        }
    }
}

pub struct CommandHandle {
    child: Arc<SharedChild>,
    done_rx: oneshot::Receiver<CommandResult<ExitStatus>>,
    out_rx: oneshot::Receiver<CommandResult<String>>,
    err_rx: oneshot::Receiver<CommandResult<String>>,
    log: OutputLog,
}

impl CommandHandle {
    pub async fn wait(self) -> CommandResult<CommandOutput> {
        let (status, out, err) = futures::join!(self.done_rx, self.out_rx, self.err_rx);
        let (status, out, err) = (status.unwrap()?, out.unwrap()?, err.unwrap()?);

        self.log.log_status(status.code())?;

        Ok(CommandOutput::new(status.code(), out, err))
    }

    pub fn child(&self) -> &Arc<SharedChild> {
        &self.child
    }
}

pub type EnvVars = LinkedHashMap<String, String>;

pub enum SourceRef<'a> {
    Path(&'a Path),
    Source(&'a str),
}

impl<'a> SourceRef<'a> {
    pub fn read(&self) -> Result<String, kg_diag::IoErrorDetail> {
        match *self {
            SourceRef::Path(path) => {
                let mut s = String::new();
                fs::read_to_string(path, &mut s)?;
                Ok(s)
            }
            SourceRef::Source(src) => Ok(src.into()),
        }
    }

    pub fn read_to(&self, buf: &mut String) -> Result<(), kg_diag::IoErrorDetail> {
        match *self {
            SourceRef::Path(path) => {
                fs::read_to_string(path, buf)?;
                Ok(())
            }
            SourceRef::Source(src) => {
                buf.push_str(src);
                Ok(())
            }
        }
    }

    pub fn to_owned(&self) -> Source {
        match self {
            SourceRef::Path(p) => Source::Path(p.to_path_buf()),
            SourceRef::Source(src) => Source::Source(src.to_string()),
        }
    }
}

pub enum Source {
    Path(PathBuf),
    Source(String),
}

impl Source {
    pub fn as_ref(&self) -> SourceRef<'_> {
        match self {
            Source::Path(p) => SourceRef::Path(p.as_path()),
            Source::Source(src) => SourceRef::Source(src.as_str()),
        }
    }
}

fn prepare_script<W: std::io::Write>(
    script: SourceRef,
    args: &[String],
    env: Option<&EnvVars>,
    cwd: Option<&Path>,
    mut out: W,
) -> Result<(), IoErrorDetail> {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    let script = script.read()?;

    writeln!(out, "#!/usr/bin/env bash")?;

    if let Some(cwd) = cwd {
        writeln!(out, "cd \"{}\"", cwd.display())?;
    }
    if let Some(env) = env {
        for (k, v) in env {
            writeln!(out, "export {}='{}'", k, v)?;
        }
    }

    // Create temp script file in ramdisk
    let tmp_path = format!("/dev/shm/op_{:0x}", rng.gen::<u64>());
    writeln!(out, "cat > {} <<-'%%EOF%%'", tmp_path)?;
    writeln!(out, "{}", script.trim())?;
    writeln!(out, "%%EOF%%")?;

    // Make temp script executable
    writeln!(out, "chmod +x {}", tmp_path)?;

    // Execute tmp script
    if args.is_empty() {
        writeln!(out, "({})", tmp_path)?;
    } else {
        write!(out, "({}", tmp_path)?;
        for arg in args {
            write!(out, " \'{}\'", arg)?;
        }
        writeln!(out, ")")?;
    }

    // Capture script status
    write!(out, "STATUS=$?\n")?;

    // Remove temp script
    write!(out, "rm -f {}\n", tmp_path)?;

    // Exit with tmp script status code
    write!(out, "exit $STATUS\n")?;

    Ok(())
}

#[derive(Debug, Clone)]
pub struct CommandBuilder {
    cmd: String,
    args: Vec<String>,
    envs: LinkedHashMap<String, String>,
    setsid: bool,
}

impl CommandBuilder {
    pub fn new<S: Into<String>>(cmd: S) -> CommandBuilder {
        CommandBuilder {
            cmd: cmd.into(),
            args: Vec::new(),
            envs: LinkedHashMap::new(),
            setsid: false,
        }
    }

    pub fn arg<S: Into<String>>(&mut self, arg: S) -> &mut CommandBuilder {
        self.args.push(arg.into());
        self
    }

    pub fn args<S: Into<String>, I: Iterator<Item = S>>(&mut self, args: I) -> &mut CommandBuilder {
        for a in args {
            self.args.push(a.into());
        }
        self
    }

    pub fn env<K: Into<String>, V: Into<String>>(
        &mut self,
        key: K,
        value: V,
    ) -> &mut CommandBuilder {
        self.envs.insert(key.into(), value.into());
        self
    }

    pub fn setsid(&mut self, enable: bool) -> &mut CommandBuilder {
        self.setsid = enable;
        self
    }

    #[cfg(unix)]
    fn handle_setsid(&self, c: &mut Command) {
        use std::os::unix::process::CommandExt;

        if self.setsid {
            unsafe {
                c.pre_exec(|| {
                    if libc::setsid() == -1 {
                        Err(std::io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }
        }
    }
    #[cfg(unix)]
    fn handle_setsid_sync(&self, c: &mut std::process::Command) {
        use std::os::unix::process::CommandExt;

        if self.setsid {
            unsafe {
                c.pre_exec(|| {
                    if libc::setsid() == -1 {
                        Err(std::io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }
        }
    }
    #[cfg(not(unix))]
    fn handle_setsid(&self, c: &mut Command) {
        if self.setsid {
            unsupported!()
        }
    }

    pub fn build(&self) -> Command {
        let mut c = Command::new(&self.cmd);
        for a in self.args.iter() {
            c.arg(a);
        }
        for (k, v) in self.envs.iter() {
            c.env(k, v);
        }
        self.handle_setsid(&mut c);
        c
    }
    // sync version of this method is necessary because we cannot call async code in SshSession destructor
    pub fn build_sync(&self) -> std::process::Command {
        let mut c = std::process::Command::new(&self.cmd);
        for a in self.args.iter() {
            c.arg(a);
        }
        for (k, v) in self.envs.iter() {
            c.env(k, v);
        }
        self.handle_setsid_sync(&mut c);
        c
    }

    /// Returns command string representation with env vars at the beginning
    /// eg. `ENV1='some value' printenv`
    pub fn to_string_with_env(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();

        let envs = self
            .envs
            .iter()
            .map(|(k, v)| format!("{}='{}'", k, v))
            .collect::<Vec<String>>()
            .join(" ");

        write!(out, "{} ", envs).unwrap();

        write!(out, "{}", self.cmd).unwrap();

        for a in self.args.iter() {
            if a.contains(' ') {
                write!(out, " \"{}\"", a).unwrap();
            } else {
                write!(out, " {}", a).unwrap();
            }
        }
        out
    }
}

impl std::fmt::Display for CommandBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.cmd)?;
        for a in self.args.iter() {
            if a.contains(' ') {
                write!(f, " \"{}\"", a)?;
            } else {
                write!(f, " {}", a)?;
            }
        }
        Ok(())
    }
}

fn collect_out<R: Read, F: FnMut(&str) -> CommandResult<()>>(
    reader: R,
    mut line_cb: F,
) -> CommandResult<String> {
    let mut out = String::new();
    let r = BufReader::new(reader);
    let lines = r.lines();

    use std::fmt::Write;

    for res in lines {
        let line = res.map_err_to_diag()?;
        line_cb(&line)?;
        writeln!(&mut out, "{}", &line).unwrap();
    }

    Ok(out)
}

fn handle_std<O: Read + Send + 'static, E: Read + Send + 'static>(
    log: &OutputLog,
    out_reader: O,
    err_reader: E,
) -> (
    oneshot::Receiver<CommandResult<String>>,
    oneshot::Receiver<CommandResult<String>>,
) {
    let l = log.clone();
    let out_rx = spawn_blocking(move || {
        collect_out(out_reader, |line| {
            l.log_out(line.as_bytes())?;
            Ok(())
        })
    });

    let l = log.clone();
    let err_rx = spawn_blocking(move || {
        collect_out(err_reader, |line| {
            l.log_err(line.as_bytes())?;
            Ok(())
        })
    });
    (out_rx, err_rx)
}

/*

async fn execute(mut command: Command, _log: &OutputLog) -> CommandResult<()> {
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(Stdio::null());

    let mut child = command.spawn().map_err_to_diag()?;

    let stdout = BufReader::new(child.stdout.take().unwrap());
    let stderr = BufReader::new(child.stderr.take().unwrap());
    drop(child.stdin.take());

    async fn status(child: Child) -> Result<(), std::io::Error> {
        let status = child.await?;
        println!("status: {}", status);
        Ok(())
    }
    handle_out(stdout, stderr).await?;
    status(child).await.map_err_to_diag()?;
    Ok(())
}

async fn execute_pty(
    command: std::process::Command,
    _log: &OutputLog,
) -> Result<(), std::io::Error> {
    let mut session = rexpect::session::spawn_command(command, None).expect("spawn");
    while let Ok(line) = session.read_line() {
        println!("out: {:?}", line);
    }
    Ok(())
}

async fn handle_out<R1: AsyncRead + Unpin, R2: AsyncRead + Unpin>(
    stdout: BufReader<R1>,
    stderr: BufReader<R2>,
) -> CommandResult<()> {
    async fn stdout_read<R: AsyncRead + Unpin>(s: BufReader<R>) -> Result<(), std::io::Error> {
        let mut stdout = lines(s);
        while let Some(line) = stdout.next_line().await? {
            println!("out: {:?}", line);
        }
        println!("out: ---");
        Ok(())
    }

    async fn stderr_read<R: AsyncRead + Unpin>(s: BufReader<R>) -> Result<(), std::io::Error> {
        let mut stderr = lines(s);
        while let Some(line) = stderr.next_line().await? {
            println!("err: {:?}", line);
        }
        println!("err: ---");
        Ok(())
    };
    try_join(stdout_read(stdout), stderr_read(stderr))
        .await
        .map_err_to_diag()?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_command() {
        let mut cmd = Command::new("/usr/bin/bash");
        cmd.arg("-c")
            .arg("for i in {1..10}; do echo stdout output; echo stderr output 1>&2;  done;");

        let log = OutputLog::new();

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async move {
            execute(cmd, &log).await.expect("error");
        });
    }

    #[test]
    fn aaa_command() {
        let mut cmd = tokio::process::Command::new("/usr/bin/rsync");
        cmd.arg("-av")
            .arg("--stats")
            .arg("--progress")
            .arg("--bwlimit=200k")
            .arg("./../target/debug/incremental")
            .arg("./../target/debug2");

        let log = OutputLog::new();

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async move {
            execute(cmd, &log).await.expect("error");
        });
    }
}
*/
