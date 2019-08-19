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
use tokio_process::CommandExt;

mod config;
mod dest;
mod operations;

#[derive(Debug)]
pub enum SshError {
    IoErrorDetail(kg_diag::IoErrorDetail),
    SshOpen(String),
    SshProcessTerminated,
    SshClosed,
    ScriptOpenError(std::io::Error),
}

//FIXME (jc)
impl From<kg_diag::IoErrorDetail> for SshError {
    fn from(err: kg_diag::IoErrorDetail) -> Self {
        SshError::IoErrorDetail(err)
    }
}

impl From<std::io::Error> for SshError {
    fn from(err: std::io::Error) -> Self {
        SshError::IoErrorDetail(err.into())
    }
}

pub type SshResult<T> = Result<T, SshError>;

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

    fn ssh_cmd(&self) -> CommandBuilder {
        let mut cmd = CommandBuilder::new(self.config().ssh_cmd());
        self.dest.set_dest(&mut cmd);

        cmd.arg("-S")
            .arg(self.socket_path.to_str().unwrap())
            .arg("-T")
            .arg("-o")
            .arg("StrictHostKeyChecking=yes");

        cmd
    }

    pub(crate) fn remote_shell_call(&self) -> String {
        let cmd = self.ssh_cmd();
        cmd.to_string()
    }

    fn open(&mut self) -> SshResult<()> {
        let mut cmd = self
            .ssh_cmd()
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

        let output = cmd.output()?;
        if output.status.success() {
            self.opened.set(true);
            Ok(())
        } else {
            Err(SshError::SshOpen(
                String::from_utf8(output.stderr).expect("non UTF-8 stderr output"),
            ))
        }
    }

    #[allow(dead_code)]
    fn check(&self) -> SshResult<bool> {
        if !self.opened.get() {
            return Err(SshError::SshClosed);
        }

        let mut cmd = self
            .ssh_cmd()
            .arg("-O")
            .arg("check")
            .arg("-o")
            .arg("ConnectTimeout=2")
            .build();

        cmd.stdout(Stdio::null()).stderr(Stdio::null());

        let s = cmd.status()?;
        Ok(s.success())
    }

    fn close(&mut self) -> SshResult<()> {
        if !self.opened.get() {
            return Ok(());
        }

        let mut cmd = self
            .ssh_cmd()
            .arg("-O")
            .arg("exit")
            .arg("-o")
            .arg("ConnectTimeout=2")
            .build();

        cmd.stdout(Stdio::null()).stderr(Stdio::piped());

        let output = cmd.output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(SshError::SshOpen(
                String::from_utf8(output.stderr).expect("non UTF-8 stderr output"),
            ))
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
            return Err(SshError::SshClosed);
        }

        let usr_cmd = CommandBuilder::new(cmd)
            .args(args.iter().map(String::as_str))
            .to_string();

        log.log_cmd(&usr_cmd)?;

        let mut ssh_cmd = self
            .ssh_cmd()
            .arg("-o")
            .arg("BatchMode=yes")
            .arg(usr_cmd)
            .build();

        ssh_cmd.stdout(stdout).stderr(stderr);

        Ok(ssh_cmd.spawn()?)
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
            return Err(SshError::SshClosed);
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
            .ssh_cmd()
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
        buf.seek(SeekFrom::Start(0))?;
        log.log_stdin(&mut buf)?;

        w_in.write_all(buf.get_ref())?;
        std::mem::drop(w_in);

        Ok(ssh_cmd.spawn()?)
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
            return Err(SshError::SshClosed);
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
            .ssh_cmd()
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
        buf.seek(SeekFrom::Start(0))?;

        w_in.write_all(buf.get_ref())?;
        std::mem::drop(w_in);

        Ok(ssh_cmd.spawn_async()?)
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
    ) -> Result<TaskResult, CommandError> {
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
    ) -> Result<TaskResult, CommandError> {
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
    ) -> Result<TaskResult, CommandError> {
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
    ) -> Result<TaskResult, CommandError> {
        self.read()
            .exec_script(engine, script, args, env, cwd, run_as, out_format, log)
    }
}

// tests without assertions, should not be executed
// may hang sometimes
/*
#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::RuntimeError;
    use tokio::prelude::future::*;
    use tokio::prelude::Stream;

    lazy_static! {
        static ref LOCK: Mutex<()> = Mutex::new(());
    }

    fn lock<'a>() -> MutexGuard<'a, ()> {
        LOCK.lock().unwrap()
    }

    fn ssh_session() -> SshSession {
        let config = ConfigRef::from_json(
            r#"{
            "exec": {
                "command": {
                    "ssh": {
                        "socket_dir": "/tmp"
                    }
                }
            }
        }"#,
        )
        .unwrap();

        let username = std::env::var("USER").unwrap();
        let auth = SshAuth::PublicKey {
            identity_file: "~/.ssh/id_rsa".into(),
        };
        let dest = SshDest::new("127.0.0.1", 22, username, auth);

        SshSession::new(dest, config)
    }

    #[test]
    fn check_master_connection() {
        let _lock = lock();

        println!("check_master_connection");
        let mut session = ssh_session();

        session.open().unwrap();
        assert!(session.check().unwrap());

        session.close().unwrap();
        assert!(!session.check().unwrap());
    }

    #[test]
    fn exec_command() {
        let _lock = lock();

        let log = OutputLog::new(Cursor::new(Vec::new()));

        let mut session = ssh_session();
        session.open().unwrap();
        let child = session
            .run_command(
                "echo",
                &vec!["\\\"${USER}\\\"".into()],
                Stdio::piped(),
                Stdio::piped(),
                &log,
            )
            .unwrap();

        let result = execute(child, Some(FileFormat::Json), None, &log).unwrap();
        session.close().unwrap();

        println!("output:\n{}", log);
        println!("result: {:?}", result);
    }

    #[test]
    fn exec_script() {
        let _lock = lock();

        let mut session = ssh_session();

        let mut envs = LinkedHashMap::new();
        envs.insert("ENV_VAR1".into(), "some value".into());
        envs.insert("ENV_VAR2".into(), "some other val - ${USER}".into());

        session.open().unwrap();

        let log = OutputLog::new(Cursor::new(Vec::new()));
        let child = session
            .run_script(
                SourceRef::Path(Path::new("../resources/files/example-script.sh")),
                &vec![
                    "param1".into(),
                    "param 2".into(),
                    "@".into(),
                    //"&@!@#".into()
                ],
                Some(&envs),
                Some(&PathBuf::from("/home")),
                Some("root"),
                Stdio::piped(),
                Stdio::piped(),
                &log,
            )
            .unwrap();

        let result = execute(child, Some(FileFormat::Json), None, &log).unwrap();
        session.close().unwrap();

        println!("output:\n{}", log);
        println!("result: {:?}", result);
    }

    #[test]
    fn run_script_async_out_streams() {
        let mut session = ssh_session();
        session.open().unwrap();
        let mut child: tokio_process::Child = session
            .run_script_async(
                SourceRef::Source("ls -al"),
                &[],
                None,
                None,
                None,
            )
            .expect("error creating session");
        let stdout = child.stdout().take().expect("Cannot get child stdout");
        let stderr = child.stderr().take().expect("Cannot get child stderr");

        let child_fut = child
            .map_err(|err| panic!("child error"))
            .map(|exit_status| println!("Exit status : {:?}", exit_status));

        let out_fut = tokio::io::lines(BufReader::new(stdout))
            .map_err(|err| panic!())
            .map(|line| Ok(format!("out: {}", line)));
        let err_fut = tokio::io::lines(BufReader::new(stderr))
            .map_err(|err| panic!())
            .map(|line| Ok(format!("err: {}", line)));

        let out_fut = out_fut
            .select(err_fut)
            .for_each(|line: Result<String, RuntimeError>| {
                eprintln!("{}", line.unwrap());
                Ok(())
            });
        tokio::run(out_fut.join(child_fut).map(|_| ()));
    }

    #[test]
    fn run_script_async_wait_output() {
        let mut session = ssh_session();
        session.open().unwrap();
        let mut child: tokio_process::Child = session
            .run_script_async(
                SourceRef::Source("ls -al;sleep 5; >&2 echo 'Error!';sleep 5; ls -al"),
                &[],
                None,
                None,
                None,
            )
            .expect("error creating session");

        let child_fut = child
            .wait_with_output()
            .map_err(|err| panic!("child error"))
            .map(|output| {
                println!(
                    "Output : {}\n {}",
                    std::str::from_utf8(&output.stdout).unwrap(),
                    std::str::from_utf8(&output.stderr).unwrap()
                )
            });

        tokio::run(child_fut.map(|_| ()));
    }

}
*/
