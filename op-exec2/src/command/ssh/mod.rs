use std::cell::Cell;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};

use os_pipe::pipe;

use super::*;
use std::io::{Seek, SeekFrom, Write};

pub use self::config::SshConfig;
pub use self::dest::{SshAuth, SshDest};
use kg_diag::io::ResultExt;

mod config;
mod dest;

pub type SshError = BasicDiag;
pub type SshResult<T> = Result<T, SshError>;

#[derive(Debug, Display, Detail)]
pub enum SshErrorDetail {
    #[display(fmt = "ssh process didn't exited successfully: {stderr}")]
    SshProcess { stderr: String },

    #[display(fmt = "connection closed")]
    SshClosed,

    #[display(fmt = "cannot spawn ssh process")]
    SshSpawn,
}

impl SshErrorDetail {
    pub fn closed<T>() -> SshResult<T> {
        Err(SshErrorDetail::SshClosed.into())
    }
    pub fn process_exit<T>(stderr: String) -> SshResult<T> {
        Err(SshErrorDetail::SshProcess { stderr }.into())
    }

    pub fn spawn_err(err: std::io::Error) -> SshError {
        let err = IoErrorDetail::from(err);
        SshErrorDetail::SshSpawn.with_cause(BasicDiag::from(err))
    }
}

#[derive(Debug)]
pub struct SshSessionCache {
    config: SshConfig,
    cache: LruCache<String, SshSessionRef>,
}

impl SshSessionCache {
    pub fn new(config: SshConfig) -> SshSessionCache {
        let capacity = config.cache_limit();
        SshSessionCache {
            config,
            cache: LruCache::new(capacity),
        }
    }

    pub fn init(&mut self) -> IoResult<()> {
        std::fs::remove_dir_all(self.config.socket_dir())?;
        fs::create_dir_all(self.config.socket_dir())?;
        Ok(())
    }

    pub async fn get(&mut self, dest: &SshDest) -> SshResult<SshSessionRef> {
        let key = dest.to_id_string();
        if let Some(s) = self.cache.get_mut(&key) {
            return Ok(s.clone());
        }

        let mut s = SshSession::new(dest.clone(), self.config.clone());
        s.open().await?;
        let s_ref = SshSessionRef::new(s);
        self.cache.insert(key, s_ref.clone());
        Ok(s_ref)
    }
}

#[derive(Debug)]
pub struct SshSession {
    opened: Cell<bool>,
    config: SshConfig,
    id: String,
    socket_path: PathBuf,
    dest: SshDest,
}

impl SshSession {
    pub fn new(dest: SshDest, config: SshConfig) -> SshSession {
        let id = dest.to_id_string();
        let socket_path = config.socket_dir().join(id.clone() + ".sock");

        SshSession {
            opened: Cell::new(false),
            config,
            id,
            socket_path,
            dest,
        }
    }

    fn config(&self) -> &SshConfig {
        &self.config
    }

    /// Returns CommandBuilder with default args.
    /// # Arguments
    /// * `include_target` - if `false` target `username@hostname` will not be set.
    fn ssh_cmd(&self, include_target: bool) -> CommandBuilder {
        let mut cmd = CommandBuilder::new(self.config().ssh_cmd());
        self.dest.set_dest(include_target, &mut cmd);

        cmd.arg("-S")
            .arg(self.socket_path.to_str().unwrap())
            .arg("-T")
            .arg("-o")
            .arg("StrictHostKeyChecking=yes");

        cmd
    }

    /// Returns ssh command string without target host and username
    pub(crate) fn remote_shell_cmd(&self) -> String {
        let cmd = self.ssh_cmd(false);
        cmd.to_string()
    }

    async fn open(&mut self) -> SshResult<()> {
        let mut cmd = self
            .ssh_cmd(true)
            .arg("-n")
            .arg("-M") //Master mode
            .arg("-N") //Do not execute a remote command
            .arg("-o")
            .arg("ControlMaster=auto")
            .arg("-o")
            .arg("ControlPersist=yes")
            .arg("-o")
            .arg("ConnectTimeout=2")
            .build();

        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let output = cmd.output().await.map_err(SshErrorDetail::spawn_err)?;
        if output.status.success() {
            self.opened.set(true);
            Ok(())
        } else {
            SshErrorDetail::process_exit(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    #[allow(dead_code)]
    async fn check(&self) -> SshResult<bool> {
        if !self.opened.get() {
            return SshErrorDetail::closed();
        }

        let mut cmd = self
            .ssh_cmd(true)
            .arg("-O")
            .arg("check")
            .arg("-o")
            .arg("ConnectTimeout=2")
            .build();

        cmd.stdout(Stdio::null()).stderr(Stdio::null());

        let s = cmd.status().await.map_err(SshErrorDetail::spawn_err)?;
        Ok(s.success())
    }

    fn close(&mut self) -> SshResult<()> {
        if !self.opened.get() {
            return Ok(());
        }

        let mut cmd = self
            .ssh_cmd(true)
            .arg("-O")
            .arg("exit")
            .arg("-o")
            .arg("ConnectTimeout=2")
            .build_sync();

        cmd.stdout(Stdio::null()).stderr(Stdio::piped());

        let output = cmd.output().map_err(SshErrorDetail::spawn_err)?;
        if output.status.success() {
            Ok(())
        } else {
            SshErrorDetail::process_exit(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    async fn run_command(
        &mut self,
        cmd: &str,
        args: &[String],
        // TODO ws is this necessary?
        // env: Option<&EnvVars>,
        // cwd: Option<&Path>,
        // run_as: Option<&str>,
        log: &OutputLog,
    ) -> SshResult<ExitStatus> {
        if !self.opened.get() {
            return SshErrorDetail::closed();
        }

        let usr_cmd = CommandBuilder::new(cmd)
            .args(args.iter().map(String::as_str))
            .to_string();

        log.log_in(usr_cmd.as_bytes())?;

        let mut ssh_cmd = self
            .ssh_cmd(true)
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(usr_cmd)
            .build();

        log.log_in(format!("{:?}", ssh_cmd).as_bytes())?;

        ssh_cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = ssh_cmd.spawn().map_err(SshErrorDetail::spawn_err)?;

        let stdout = BufReader::new(child.stdout.take().unwrap());
        let stderr = BufReader::new(child.stderr.take().unwrap());
        drop(child.stdin.take());

        // TODO ws handle stdout and stderr
        handle_out(stdout, stderr).await?;

        let status = child.await.map_err_to_diag()?;
        log.log_status(status.code())?;

        Ok(status)
    }

    async fn run_script(
        &mut self,
        script: SourceRef<'_>,
        args: &[String],
        env: Option<&EnvVars>,
        cwd: Option<&Path>,
        run_as: Option<&str>,
        log: &OutputLog,
    ) -> SshResult<ExitStatus> {
        if !self.opened.get() {
            return SshErrorDetail::closed();
        }

        let mut builder = if let Some(user) = run_as {
            let mut cmd = CommandBuilder::new(self.config().runas_cmd());
            cmd.arg("-u").arg(user).arg(self.config().shell_cmd());
            cmd
        } else {
            let cmd = CommandBuilder::new(self.config().shell_cmd());
            cmd
        };

        builder.arg("/dev/stdin");

        let usr_cmd = builder.to_string();

        let (r_in, mut w_in) = pipe().unwrap();
        let _r = r_in.try_clone().unwrap();
        log.log_in(usr_cmd.as_bytes())?;

        let mut ssh_cmd = self
            .ssh_cmd(true)
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(usr_cmd)
            .build();

        ssh_cmd
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::from(r_in));

        let mut buf = Cursor::new(Vec::new());
        prepare_script(script, args, env, cwd, &mut buf)?;
        buf.seek(SeekFrom::Start(0)).map_err_to_diag()?;

        w_in.write_all(buf.get_ref()).map_err_to_diag()?;
        std::mem::drop(w_in);
        let mut child = ssh_cmd.spawn().map_err(SshErrorDetail::spawn_err)?;

        let stdout = BufReader::new(child.stdout.take().unwrap());
        let stderr = BufReader::new(child.stderr.take().unwrap());

        handle_out(stdout, stderr).await?;

        let status = child.await.map_err_to_diag()?;
        Ok(status)
    }
}

impl Drop for SshSession {
    fn drop(&mut self) {
        // eprintln!("Closing ssh session");
        if let Err(err) = self.close() {
            eprintln!("Error closing ssh connection! {:?}", err);
        }
    }
}

#[derive(Debug, Clone)]
pub struct SshSessionRef(Arc<Mutex<SshSession>>);

impl SshSessionRef {
    fn new(session: SshSession) -> SshSessionRef {
        SshSessionRef(Arc::new(Mutex::new(session)))
    }

    pub fn read(&self) -> MutexGuard<SshSession> {
        self.0.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_shell_cmd() {
        let dest = SshDest::new(
            "127.0.0.1",
            8821,
            "root",
            SshAuth::PublicKey {
                identity_file: PathBuf::from("keys/vagrant"),
            },
        );
        let config = SshConfig::default();
        let sess = SshSession::new(dest, config);

        let cmd = sess.remote_shell_cmd();

        eprintln!("cmd = {}", cmd);
    }

    #[test]
    fn run_command_test() {
        let auth = SshAuth::PublicKey {
            identity_file: "/home/wiktor/.ssh/id_rsa".into(),
        };
        let dest = SshDest::new("localhost", 22, "wiktor", auth);
        let cfg = SshConfig::default();

        let mut sess = SshSession::new(dest, cfg);

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async move {
            sess.open().await.expect("Cannot open session");
            let log = OutputLog::new();

            let status = sess
                .run_command("ls", &["-al".into()], &log)
                .await
                .expect("Error");
            eprintln!("status = {:?}", status);
            eprintln!("log = {}", log);
        });
    }

    #[test]
    fn run_script_test() {
        let auth = SshAuth::PublicKey {
            identity_file: "/home/wiktor/.ssh/id_rsa".into(),
        };
        let dest = SshDest::new("localhost", 22, "wiktor", auth);
        let cfg = SshConfig::default();

        let mut sess = SshSession::new(dest, cfg);

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        let script = SourceRef::Source(
            r#"
        echo 'printing cwd'
        pwd

        echo 'printing arguments...'
        echo $@

        echo "listing files..."
        ls -al

        echo 'Printing $TEST_ENV_VAR ...'
        echo $TEST_ENV_VAR
        exit 2

        "#,
        );

        let mut env = EnvVars::new();

        env.insert(
            "TEST_ENV_VAR".into(),
            "This is environment variable content ".into(),
        );

        rt.block_on(async move {
            sess.open().await.expect("Error opening session");
            let log = OutputLog::new();

            let status = sess
                .run_script(
                    script,
                    &["-some_argument".into()],
                    Some(&env),
                    Some(&PathBuf::from("/home")),
                    None,
                    &log,
                )
                .await
                .expect("Error");
            eprintln!("status = {:?}", status);
            eprintln!("log = {}", log);
        });
    }
}
