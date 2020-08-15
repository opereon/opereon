use crate::ops::SpawnableCommand;
use crate::outcome::Outcome;
use crate::utils::SharedChildExt;
use async_trait::*;
use op_engine::operation::OperationResult;
use op_engine::{EngineRef, OperationImpl, OperationRef};
use op_exec::command::ssh::{SshDest, SshSessionCacheRef};
use op_exec::command::{CommandHandle, EnvVars, Source, SourceRef};
use op_exec::OutputLog;
use std::path::{Path, PathBuf};

pub struct SshCommandOperation {
    cmd: String,
    args: Vec<String>,
    env: Option<EnvVars>,
    log: OutputLog,

    dest: SshDest,
    cache: SshSessionCacheRef,
}

impl SshCommandOperation {
    pub fn new(
        cmd: &str,
        args: &[String],
        env: Option<&EnvVars>,
        log: &OutputLog,
        dest: &SshDest,
        cache: &SshSessionCacheRef,
    ) -> Self {
        SshCommandOperation {
            cmd: cmd.to_string(),
            args: args.to_vec(),
            env: env.cloned(),
            log: log.clone(),

            dest: dest.clone(),
            cache: cache.clone(),
        }
    }
}

#[async_trait]
impl SpawnableCommand for SshCommandOperation {
    async fn spawn(&self) -> OperationResult<CommandHandle> {
        let sess = self.cache.lock().await.get(&self.dest).await?;

        let mut s = sess.lock().await;
        s.spawn_command(&self.cmd, &self.args, self.env.as_ref(), &self.log)
    }
}
command_operation_impl!(SshCommandOperation);

pub struct SshScriptOperation {
    script: Source,
    args: Vec<String>,
    env: Option<EnvVars>,
    cwd: Option<PathBuf>,
    run_as: Option<String>,
    log: OutputLog,

    dest: SshDest,
    cache: SshSessionCacheRef,
}

impl SshScriptOperation {
    pub fn new(
        script: SourceRef<'_>,
        args: &[String],
        env: Option<&EnvVars>,
        cwd: Option<&Path>,
        run_as: Option<&str>,
        log: &OutputLog,
        dest: &SshDest,
        cache: &SshSessionCacheRef,
    ) -> Self {
        SshScriptOperation {
            script: script.to_owned(),
            args: args.to_vec(),
            env: env.cloned(),
            cwd: cwd.map(|c| c.to_path_buf()),
            run_as: run_as.map(|r| r.to_string()),
            log: log.clone(),
            dest: dest.clone(),
            cache: cache.clone(),
        }
    }
}

#[async_trait]
impl SpawnableCommand for SshScriptOperation {
    async fn spawn(&self) -> OperationResult<CommandHandle> {
        let sess = self.cache.lock().await.get(&self.dest).await?;

        let mut s = sess.lock().await;
        s.spawn_script(
            self.script.as_ref(),
            &self.args,
            self.env.as_ref(),
            self.cwd.as_deref(),
            self.run_as.as_deref(),
            &self.log,
        )
    }
}
command_operation_impl!(SshScriptOperation);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outcome::Outcome;
    use op_engine::operation::OperationImplExt;
    use op_engine::{EngineRef, OperationRef};
    use op_exec::command::ssh::{SshAuth, SshConfig};
    use op_exec::command::SourceRef;
    use std::path::PathBuf;

    #[test]
    fn ssh_command_operation_test() {
        let auth = SshAuth::PublicKey {
            identity_file: "/home/wiktor/.ssh/id_rsa".into(),
        };
        let dest = SshDest::new("localhost", 22, "wiktor", auth);

        let engine: EngineRef<Outcome> = EngineRef::default();
        let mut rt = EngineRef::<()>::build_runtime();

        let mut cfg = SshConfig::default();
        cfg.set_socket_dir(&PathBuf::from("/home/wiktor/.ssh/connections"));

        let cache = SshSessionCacheRef::new(cfg);

        let mut env = EnvVars::new();

        env.insert(
            "TEST_ENV_VAR".into(),
            "This is environment variable content ".into(),
        );

        let log = OutputLog::new();

        let op_impl = SshCommandOperation::new(
            "ls",
            &["-a".into(), "-l".into()],
            Some(&env),
            &log,
            &dest,
            &cache,
        );
        let op = OperationRef::new("ssh_command", op_impl.boxed());

        rt.block_on(async move {
            let e = engine.clone();

            tokio::spawn(async move {
                let res = engine.enqueue_with_res(op).await.unwrap();
                println!("operation completed {:?}", res);
                eprintln!("log = {}", log);
                engine.stop();
            });
            e.start().await;
        })
    }

    #[test]
    fn ssh_script_operation_test() {
        let auth = SshAuth::PublicKey {
            identity_file: "/home/wiktor/.ssh/id_rsa".into(),
        };
        let dest = SshDest::new("localhost", 22, "wiktor", auth);

        let engine: EngineRef<Outcome> = EngineRef::default();
        let mut rt = EngineRef::<()>::build_runtime();

        let mut cfg = SshConfig::default();
        cfg.set_socket_dir(&PathBuf::from("/home/wiktor/.ssh/connections"));

        let cache = SshSessionCacheRef::new(cfg);
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

        let log = OutputLog::new();

        let op_impl = SshScriptOperation::new(
            script,
            &["-a".into(), "-l".into()],
            Some(&env),
            Some(&PathBuf::from("/home")),
            None,
            &log,
            &dest,
            &cache,
        );
        let op = OperationRef::new("ssh_command", op_impl.boxed());

        rt.block_on(async move {
            let e = engine.clone();

            tokio::spawn(async move {
                let res = engine.enqueue_with_res(op).await.unwrap();
                println!("operation completed {:?}", res);
                eprintln!("log = {}", log);
                engine.stop();
            });
            e.start().await;
        })
    }
}
