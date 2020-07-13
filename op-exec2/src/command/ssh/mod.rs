use std::cell::Cell;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Stdio};
use std::sync::{Arc};

use os_pipe::pipe;

use super::*;
use std::io::{Seek, SeekFrom, Write};

pub use self::config::SshConfig;
pub use self::dest::{SshAuth, SshDest};
use crate::utils::spawn_blocking;
use kg_diag::io::fs::create_dir_all;
use kg_diag::io::ResultExt;
use shared_child::SharedChild;
use futures::lock::{Mutex, MutexGuard};

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

    #[display(fmt = "cannot create master socket directory")]
    SocketDir,
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

    pub async fn init(&mut self) -> SshResult<()> {
        // std::fs::remove_dir_all(self.config.socket_dir())?;
        let socket_dir = self.config.socket_dir().to_path_buf();
        let done_rx = spawn_blocking(move || {
            fs::create_dir_all(socket_dir)
                .into_diag_res()
                .map_err_as_cause(|| SshErrorDetail::SocketDir)
        });
        done_rx.await.unwrap()?;
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

#[derive(Debug, Clone)]
pub struct SshSessionCacheRef(Arc<Mutex<SshSessionCache>>);

impl SshSessionCacheRef {
    pub fn new(config: SshConfig) -> Self {
        let cache = SshSessionCache::new(config);
        SshSessionCacheRef(Arc::new(Mutex::new(cache)))
    }

    pub fn from_cache(cache: SshSessionCache) -> Self {
        SshSessionCacheRef(Arc::new(Mutex::new(cache)))
    }

    pub async fn lock(&self) -> MutexGuard<'_, SshSessionCache> {
        self.0.lock().await
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
        if self.opened.get() {
            // Without this check multiple calls to this function will cause deadlock at filesystem level.
            // Ssh will hang if socket already exists
            return Ok(());
        }
        let sock_dir = self.config.socket_dir().to_owned();
        let sock_dir_res = spawn_blocking(move || {
            create_dir_all(sock_dir)
                .into_diag_res()
                .map_err_as_cause(|| SshErrorDetail::SocketDir)
        });
        sock_dir_res.await.unwrap()?;

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

        let done_rx = spawn_blocking(move || {
            let output = cmd.output().map_err(SshErrorDetail::spawn_err);
            output
        });
        let output = done_rx.await.unwrap()?;
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

        let done_rx = spawn_blocking(move || cmd.status().map_err(SshErrorDetail::spawn_err));

        let s = done_rx.await.unwrap()?;
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

    pub fn spawn_command(
        &mut self,
        cmd: &str,
        args: &[String],
        env: Option<&EnvVars>,
        // TODO ws is this necessary?
        // cwd: Option<&Path>,
        // run_as: Option<&str>,
        log: &OutputLog,
    ) -> SshResult<CommandHandle> {
        if !self.opened.get() {
            return SshErrorDetail::closed();
        }
        let mut builder = CommandBuilder::new(cmd);

        if let Some(envs) = env {
            for (k, v) in envs {
                builder.env(k, v);
            }
        }

        let usr_cmd = builder
            .args(args.iter().map(String::as_str))
            .to_string_with_env();

        let mut ssh_cmd = self
            .ssh_cmd(true)
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(usr_cmd)
            .build();

        let (out_reader, out_writer) = pipe().unwrap();
        let (err_reader, err_writer) = pipe().unwrap();

        ssh_cmd
            .stdin(Stdio::null())
            .stdout(out_writer)
            .stderr(err_writer);

        log.log_in(format!("{:?}", ssh_cmd).as_bytes())?;

        let child = SharedChild::spawn(&mut ssh_cmd).map_err(SshErrorDetail::spawn_err)?;
        drop(ssh_cmd);
        let child = Arc::new(child);

        let (out_rx, err_rx) = handle_std(log, out_reader, err_reader);

        let c = child.clone();
        let done_rx = spawn_blocking(move || c.wait().map_err(CommandErrorDetail::spawn_err));

        Ok(CommandHandle {
            child,
            done_rx,
            out_rx,
            err_rx,
            log: log.clone(),
        })
    }

    pub fn spawn_script(
        &mut self,
        script: SourceRef<'_>,
        args: &[String],
        env: Option<&EnvVars>,
        cwd: Option<&Path>,
        run_as: Option<&str>,
        log: &OutputLog,
    ) -> SshResult<CommandHandle> {
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

        let (in_reader, mut in_writer) = pipe().unwrap();
        let (out_reader, out_writer) = pipe().unwrap();
        let (err_reader, err_writer) = pipe().unwrap();

        let _r = in_reader.try_clone().unwrap();

        let mut ssh_cmd = self
            .ssh_cmd(true)
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(usr_cmd)
            .build();

        ssh_cmd
            .stdout(out_writer)
            .stderr(err_writer)
            .stdin(in_reader);

        log.log_in(format!("{:?}", ssh_cmd).as_bytes())?;

        let mut buf = Cursor::new(Vec::new());
        prepare_script(script, args, env, cwd, &mut buf)?;
        buf.seek(SeekFrom::Start(0)).map_err_to_diag()?;

        log.log_in(buf.get_ref().as_slice())?;

        in_writer.write_all(buf.get_ref()).map_err_to_diag()?;
        std::mem::drop(in_writer);

        let child = SharedChild::spawn(&mut ssh_cmd).map_err(SshErrorDetail::spawn_err)?;
        drop(ssh_cmd);
        let child = Arc::new(child);

        let (out_rx, err_rx) = handle_std(log, out_reader, err_reader);

        let c = child.clone();
        let done_rx = spawn_blocking(move || c.wait().map_err(CommandErrorDetail::spawn_err));
        Ok(CommandHandle {
            child,
            done_rx,
            out_rx,
            err_rx,
            log: log.clone(),
        })
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

    pub async fn lock(&self) -> MutexGuard<'_, SshSession> {
        self.0.lock().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use op_test_helpers::UnwrapDisplay;
    use tokio::time::Duration;

    #[test]
    fn cancel_command_test() {
        let auth = SshAuth::PublicKey {
            identity_file: "/home/wiktor/.ssh/id_rsa".into(),
        };
        let dest = SshDest::new("localhost", 22, "wiktor", auth);
        let mut cfg = SshConfig::default();
        cfg.set_socket_dir(&PathBuf::from("/home/wiktor/.ssh/connections"));

        let mut sess = SshSession::new(dest, cfg);

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async move {
            sess.open().await.unwrap_disp();
            let log = OutputLog::new();

            let handle = sess
                .spawn_command("ls -alR", &["/".into()], None, &log)
                .unwrap_disp();

            let child = handle.child().clone();

            tokio::spawn(async move {
                println!("Waiting...");
                tokio::time::delay_for(Duration::from_secs(1)).await;
                println!("killing command...");
                child.kill().unwrap();
                println!("signal sent");
            });

            let out = handle.wait().await.unwrap_disp();

            eprintln!("status = {:?}", out);
            eprintln!("log = {}", log);
        });
    }

    #[test]
    fn run_command_test() {
        let auth = SshAuth::PublicKey {
            identity_file: "/home/wiktor/.ssh/id_rsa".into(),
        };
        let dest = SshDest::new("localhost", 22, "wiktor", auth);
        let mut cfg = SshConfig::default();
        cfg.set_socket_dir(&PathBuf::from("/home/wiktor/.ssh/connections"));

        let mut sess = SshSession::new(dest, cfg);

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async move {
            sess.open().await.unwrap_disp();
            let log = OutputLog::new();

            let handle = sess
                .spawn_command("ls", &["-al".into()], None, &log)
                .unwrap_disp();

            let out = handle.wait().await.unwrap_disp();

            eprintln!("status = {:?}", out);
            eprintln!("log = {}", log);
        });
    }

    #[test]
    fn run_command_env_test() {
        let auth = SshAuth::PublicKey {
            identity_file: "/home/wiktor/.ssh/id_rsa".into(),
        };
        let dest = SshDest::new("localhost", 22, "wiktor", auth);
        let mut cfg = SshConfig::default();
        cfg.set_socket_dir(&PathBuf::from("/home/wiktor/.ssh/connections"));

        let mut sess = SshSession::new(dest, cfg);

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        let mut env = EnvVars::new();

        env.insert(
            "TEST_ENV_VAR1".into(),
            "This is environment variable content".into(),
        );

        env.insert(
            "TEST_ENV_VAR2".into(),
            "Another variable content".into(),
        );

        rt.block_on(async move {
            sess.open().await.unwrap_disp();
            let log = OutputLog::new();

            let handle = sess
                .spawn_command("printenv", &[], Some(&env), &log)
                .unwrap_disp();

            let out = handle.wait().await.unwrap_disp();

            eprintln!("status = {:?}", out);
            eprintln!("log = {}", log);
        });
    }

    #[test]
    fn run_script_test() {
        let auth = SshAuth::PublicKey {
            identity_file: "/home/wiktor/.ssh/id_rsa".into(),
        };
        let dest = SshDest::new("localhost", 22, "wiktor", auth);
        let mut cfg = SshConfig::default();
        cfg.set_socket_dir(&PathBuf::from("/home/wiktor/.ssh/connections"));

        let mut sess = SshSession::new(dest, cfg);

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        let script = SourceRef::Source(
            r#"
        echo 'printing cwd'
        pwd

        echo 'Printing to stderr...'
        echo 'This should go to stderr' >&2

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
            sess.open().await.unwrap_disp();
            let log = OutputLog::new();

            let handle = sess
                .spawn_script(
                    script,
                    &["-some_argument".into()],
                    Some(&env),
                    Some(&PathBuf::from("/home")),
                    None,
                    &log,
                )
                .unwrap_disp();

            let out = handle.wait().await.unwrap_disp();

            eprintln!("status = {:?}", out);
            eprintln!("log = {}", log);
        });
    }
}
