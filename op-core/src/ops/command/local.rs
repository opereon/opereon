use crate::ops::SpawnableCommand;
use crate::outcome::Outcome;
use crate::utils::SharedChildExt;
use async_trait::*;
use op_engine::operation::OperationResult;
use op_engine::{EngineRef, OperationImpl, OperationRef};
use op_exec::command::local::config::LocalConfig;
use op_exec::command::local::{spawn_local_command, spawn_local_script};
use op_exec::command::Source;
use op_exec::command::{CommandHandle, EnvVars, SourceRef};
use op_exec::OutputLog;
use std::path::{Path, PathBuf};

pub struct LocalCommandOperation {
    cmd: String,
    args: Vec<String>,
    env: Option<EnvVars>,
    cwd: Option<PathBuf>,
    run_as: Option<String>,
    config: LocalConfig,
    log: OutputLog,
}

impl LocalCommandOperation {
    pub fn new(
        cmd: &str,
        args: &[String],
        env: Option<&EnvVars>,
        cwd: Option<&Path>,
        run_as: Option<&str>,
        config: &LocalConfig,
        log: &OutputLog,
    ) -> Self {
        LocalCommandOperation {
            cmd: cmd.to_string(),
            args: args.to_vec(),
            env: env.cloned(),
            cwd: cwd.map(|p| p.to_owned()),
            run_as: run_as.map(|r| r.to_owned()),
            config: config.clone(),
            log: log.clone(),
        }
    }
}
#[async_trait]
impl SpawnableCommand for LocalCommandOperation {
    async fn spawn(&self) -> OperationResult<CommandHandle> {
        spawn_local_command(
            &self.cmd,
            &self.args,
            self.env.as_ref(),
            self.cwd.as_deref(),
            self.run_as.as_ref().map(|s| s.as_ref()),
            &self.config,
            &self.log,
        )
    }
}

command_operation_impl!(LocalCommandOperation);

pub struct LocalScriptOperation {
    script: Source,
    args: Vec<String>,
    env: Option<EnvVars>,
    cwd: Option<PathBuf>,
    run_as: Option<String>,
    config: LocalConfig,
    log: OutputLog,
}

impl LocalScriptOperation {
    pub fn new(
        script: SourceRef<'_>,
        args: &[String],
        env: Option<&EnvVars>,
        cwd: Option<&Path>,
        run_as: Option<&str>,
        config: &LocalConfig,
        log: &OutputLog,
    ) -> Self {
        LocalScriptOperation {
            script: script.to_owned(),
            args: args.to_vec(),
            env: env.cloned(),
            cwd: cwd.map(|p| p.to_owned()),
            run_as: run_as.map(|r| r.to_owned()),
            config: config.clone(),
            log: log.clone(),
        }
    }
}
#[async_trait]
impl SpawnableCommand for LocalScriptOperation {
    async fn spawn(&self) -> OperationResult<CommandHandle> {
        spawn_local_script(
            self.script.as_ref(),
            &self.args,
            self.env.as_ref(),
            self.cwd.as_deref(),
            self.run_as.as_ref().map(|s| s.as_ref()),
            &self.config,
            &self.log,
        )
    }
}
command_operation_impl!(LocalScriptOperation);

#[cfg(test)]
mod tests {
    use super::*;
    use op_engine::operation::OperationImplExt;

    #[test]
    fn local_command_operation_test() {
        let engine: EngineRef<Outcome> = EngineRef::default();
        let mut rt = EngineRef::<()>::build_runtime();

        let cfg = LocalConfig::default();

        let mut env = EnvVars::new();

        env.insert(
            "TEST_ENV_VAR".into(),
            "This is environment variable content ".into(),
        );

        let log = OutputLog::new();

        let op_impl = LocalCommandOperation::new(
            "ls",
            &["-a".into(), "-l".into()],
            Some(&env),
            Some(&PathBuf::from("/home")),
            None,
            &cfg,
            &log,
        );
        let op = OperationRef::new("local_command", op_impl.boxed());

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
    fn local_script_operation_test() {
        let engine: EngineRef<Outcome> = EngineRef::default();
        let mut rt = EngineRef::<()>::build_runtime();

        let cfg = LocalConfig::default();

        let mut env = EnvVars::new();

        env.insert(
            "TEST_ENV_VAR".into(),
            "This is environment variable content ".into(),
        );

        let log = OutputLog::new();

        let script = SourceRef::Source(
            r#"
        echo 'printing cwd'
        pwd

        echo 'Printing to stderr...'
        echo 'This should go to stderr' >&2

        echo 'printing arguments...'
        echo $@
        echo $1
        echo $2

        echo "listing files..."
        ls -al

        echo 'Printing $TEST_ENV_VAR ...'
        echo $TEST_ENV_VAR
        exit 2

        "#,
        );

        let op_impl = LocalScriptOperation::new(
            script,
            &["--param1".into(), "--param2".into()],
            Some(&env),
            Some(&PathBuf::from("/home")),
            None,
            &cfg,
            &log,
        );
        let op = OperationRef::new("local_script", op_impl.boxed());

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
