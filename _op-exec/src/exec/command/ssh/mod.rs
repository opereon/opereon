use std::cell::Cell;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};

use os_pipe::pipe;

use super::*;

pub use self::config::SshConfig;
pub use self::dest::{SshAuth, SshDest};
pub use self::operations::RemoteCommandOperation;
use kg_diag::io::ResultExt;
use tokio_process::CommandExt;

mod config;
mod dest;
mod operations;

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

    #[display(fmt = "cannot parse hosts opath expression")]
    HostsOpathParse,

    #[display(fmt = "cannot parse hosts definition")]
    HostsDefParse,
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
    config: ConfigRef,
    cache: LruCache<String, SshSessionRef>,
}

impl SshSessionCache {
    pub fn new(config: ConfigRef) -> SshSessionCache {
        let capacity = config.exec().command().ssh().cache_limit();
        SshSessionCache {
            config,
            cache: LruCache::new(capacity),
        }
    }

    pub fn init(&mut self) -> IoResult<()> {
        std::fs::remove_dir_all(self.config.exec().command().ssh().socket_dir())?;
        fs::create_dir_all(self.config.exec().command().ssh().socket_dir())?;
        Ok(())
    }

    pub fn get(&mut self, dest: &SshDest) -> SshResult<SshSessionRef> {
        let key = dest.to_id_string();
        if let Some(s) = self.cache.get_mut(&key) {
            return Ok(s.clone());
        }

        let mut s = SshSession::new(dest.clone(), self.config.clone());
        s.open()?;
        let s_ref = SshSessionRef::new(s);
        self.cache.insert(key, s_ref.clone());
        Ok(s_ref)
    }
}

#[derive(Debug)]
pub struct SshSession {
    opened: Cell<bool>,
    config: ConfigRef,
    id: String,
    socket_path: PathBuf,
    dest: SshDest,
}

impl SshSession {
    pub fn new(dest: SshDest, config: ConfigRef) -> SshSession {
        let id = dest.to_id_string();
        let socket_path = config
            .exec()
            .command()
            .ssh()
            .socket_dir()
            .join(id.clone() + ".sock");

        SshSession {
            opened: Cell::new(false),
            config,
            id,
            socket_path,
            dest,
        }
    }

    fn config(&self) -> &SshConfig {
        self.config.exec().command().ssh()
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

    fn open(&mut self) -> SshResult<()> {
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

        let output = cmd.output().map_err(SshErrorDetail::spawn_err)?;
        if output.status.success() {
            self.opened.set(true);
            Ok(())
        } else {
            SshErrorDetail::process_exit(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    #[allow(dead_code)]
    fn check(&self) -> SshResult<bool> {
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

        let s = cmd.status().map_err(SshErrorDetail::spawn_err)?;
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
            .build();

        cmd.stdout(Stdio::null()).stderr(Stdio::piped());

        let output = cmd.output().map_err(SshErrorDetail::spawn_err)?;
        if output.status.success() {
            Ok(())
        } else {
            SshErrorDetail::process_exit(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }

    fn run_command(
        &mut self,
        cmd: &str,
        args: &[String],
        stdout: Stdio,
        stderr: Stdio,
        log: &OutputLog,
    ) -> Result<Child, SshError> {
        if !self.opened.get() {
            return SshErrorDetail::closed();
        }

        let usr_cmd = CommandBuilder::new(cmd)
            .args(args.iter().map(String::as_str))
            .to_string();

        log.log_cmd(&usr_cmd)?;

        let mut ssh_cmd = self
            .ssh_cmd(true)
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(usr_cmd)
            .build();

        ssh_cmd.stdout(stdout).stderr(stderr);

        let res = ssh_cmd.spawn().map_err(SshErrorDetail::spawn_err)?;
        Ok(res)
    }

    // this code may be used during migration to non-blocking api
    //    fn run_command_async(&mut self,
    //                         cmd: &str,
    //                         args: &[String],
    //    ) -> Result<tokio_process::Child, SshError>{
    //        if !self.opened.get() {
    //            return Err(SshError::SshClosed);
    //        }
    //
    //        let usr_cmd = CommandBuilder::new(cmd).args(args.iter().map(String::as_str)).to_string();
    //
    //        let mut ssh_cmd = self.ssh_cmd()
    //            .arg("-o").arg("BatchMode=yes")
    //            .arg("-t")
    //            .arg(usr_cmd)
    //            .build();
    //
    //        ssh_cmd.stdout(Stdio::piped());
    //        ssh_cmd.stderr(Stdio::piped());
    //
    //        Ok(ssh_cmd.spawn_async()?)
    //    }

    fn run_script(
        &mut self,
        script: SourceRef,
        args: &[String],
        env: Option<&EnvVars>,
        cwd: Option<&Path>,
        run_as: Option<&str>,
        stdout: Stdio,
        stderr: Stdio,
        log: &OutputLog,
    ) -> Result<Child, SshError> {
        if !self.opened.get() {
            return SshErrorDetail::closed();
        }

        let mut usr_cmd = if let Some(user) = run_as {
            let mut cmd = CommandBuilder::new(self.config().runas_cmd());
            cmd.arg("-u").arg(user).arg(self.config().shell_cmd());
            cmd
        } else {
            let cmd = CommandBuilder::new(self.config().shell_cmd());
            cmd
        };

        usr_cmd.arg("/dev/stdin");

        let usr_cmd = usr_cmd.to_string();
        log.log_cmd(&usr_cmd)?;

        let (r_in, mut w_in) = pipe().unwrap();
        let _r = r_in.try_clone().unwrap();

        let mut ssh_cmd = self
            .ssh_cmd(true)
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(usr_cmd)
            .build();

        ssh_cmd
            .stdout(stdout)
            .stderr(stderr)
            .stdin(Stdio::from(r_in));

        let mut buf = Cursor::new(Vec::new());
        prepare_script(script, args, env, cwd, &mut buf)?;
        buf.seek(SeekFrom::Start(0)).map_err_to_diag()?;
        log.log_stdin(&mut buf)?;

        w_in.write_all(buf.get_ref()).map_err_to_diag()?;
        std::mem::drop(w_in);

        let res = ssh_cmd.spawn().map_err(SshErrorDetail::spawn_err)?;
        Ok(res)
    }

    fn run_script_async(
        &mut self,
        script: SourceRef,
        args: &[String],
        env: Option<&EnvVars>,
        cwd: Option<&Path>,
        run_as: Option<&str>,
    ) -> Result<tokio_process::Child, SshError> {
        if !self.opened.get() {
            return SshErrorDetail::closed();
        }

        let mut usr_cmd = if let Some(user) = run_as {
            let mut cmd = CommandBuilder::new(self.config().runas_cmd());
            cmd.arg("-u").arg(user).arg(self.config().shell_cmd());
            cmd
        } else {
            let cmd = CommandBuilder::new(self.config().shell_cmd());
            cmd
        };

        usr_cmd.arg("/dev/stdin");

        let usr_cmd = usr_cmd.to_string();

        let (r_in, mut w_in) = pipe().unwrap();
        let _r = r_in.try_clone().unwrap();

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
        let res = ssh_cmd.spawn_async().map_err(SshErrorDetail::spawn_err)?;
        Ok(res)
    }
}

impl CommandExecutor for SshSession {
    fn exec_command(
        &mut self,
        _engine: &EngineRef,
        cmd: &str,
        args: &[String],
        out_format: Option<FileFormat>,
        log: &OutputLog,
    ) -> CommandResult<TaskResult> {
        let child = self.run_command(cmd, args, Stdio::piped(), Stdio::piped(), log)?;
        execute(child, out_format, None, log)
    }

    fn exec_script(
        &mut self,
        _engine: &EngineRef,
        script: SourceRef,
        args: &[String],
        env: Option<&EnvVars>,
        cwd: Option<&Path>,
        run_as: Option<&str>,
        out_format: Option<FileFormat>,
        log: &OutputLog,
    ) -> CommandResult<TaskResult> {
        let child = self.run_script(
            script,
            args,
            env,
            cwd,
            run_as,
            Stdio::piped(),
            Stdio::piped(),
            log,
        )?;
        execute(child, out_format, None, log)
    }
}

impl Drop for SshSession {
    fn drop(&mut self) {
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

impl CommandExecutor for SshSessionRef {
    fn exec_command(
        &mut self,
        engine: &EngineRef,
        cmd: &str,
        args: &[String],
        out_format: Option<FileFormat>,
        log: &OutputLog,
    ) -> CommandResult<TaskResult> {
        self.read().exec_command(engine, cmd, args, out_format, log)
    }

    fn exec_script(
        &mut self,
        engine: &EngineRef,
        script: SourceRef,
        args: &[String],
        env: Option<&EnvVars>,
        cwd: Option<&Path>,
        run_as: Option<&str>,
        out_format: Option<FileFormat>,
        log: &OutputLog,
    ) -> CommandResult<TaskResult> {
        self.read()
            .exec_script(engine, script, args, env, cwd, run_as, out_format, log)
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
        let config = ConfigRef::default();
        let sess = SshSession::new(dest, config);

        let cmd = sess.remote_shell_cmd();

        eprintln!("cmd = {}", cmd);
    }
}