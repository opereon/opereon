use super::*;

use futures::future::try_join;
use kg_diag::io::ResultExt;

use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};

use utils::lines;

mod local;
mod ssh;

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

    write!(out, "#!/usr/bin/env bash\n")?;

    if let Some(cwd) = cwd {
        write!(out, "cd \"{}\"\n", cwd.display())?;
    }
    if let Some(env) = env {
        for (k, v) in env {
            write!(out, "export {}='{}'\n", k, v)?;
        }
    }

    // Create temp script file in ramdisk
    let tmp_path = format!("/dev/shm/op_{:0x}", rng.gen::<u64>());
    write!(out, "cat > {} <<-'%%EOF%%'\n", tmp_path)?;
    write!(out, "{}\n", script.trim())?;
    write!(out, "%%EOF%%\n")?;

    // Make temp script executable
    write!(out, "chmod +x {}\n", tmp_path)?;

    // Execute tmp script
    if args.is_empty() {
        write!(out, "({})\n", tmp_path)?;
    } else {
        write!(out, "({}", tmp_path)?;
        for arg in args {
            write!(out, " \'{}\'", arg)?;
        }
        write!(out, ")\n")?;
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
