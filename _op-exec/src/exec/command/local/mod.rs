use super::*;

pub use self::config::LocalConfig;

mod config;

#[derive(Debug)]
pub struct LocalExecutor {
    config: ConfigRef,
}

impl LocalExecutor {
    pub fn new(config: ConfigRef) -> LocalExecutor {
        LocalExecutor { config }
    }
    //
    //    fn config(&self) -> &LocalConfig {
    //        self.config.exec().command().local()
    //    }

    /*fn exec_command_impl(&mut self,
                         cmd: &str,
                         args: &[String],
                         cwd: Option<&Path>,
                         run_as: Option<&str>,
                         output: &mut Write,
                         stdout: Stdio,
                         stderr: Stdio) -> Result<ExitStatus, SshError>
    {
        /*let mut usr_cmd = CommandBuilder::new(self.config().shell_cmd());
        if let Some(user) = run_as {
            usr_cmd.run_as(self.config().runas_cmd(), user);
        }
        if let Some(cwd) = cwd {
            usr_cmd.cwd(self.config().cd_cmd(), cwd.to_str().unwrap());
        }

        usr_cmd.arg(cmd);
        for a in args.iter() {
            usr_cmd.arg(a);
        }

        log_cmd(&usr_cmd.to_string(), output)?;

        let mut cmd = usr_cmd.to_command();
        cmd
            .stdout(stdout)
            .stderr(stderr);

        let status = cmd.status()?;

        log_exit_code(status.code(), output)?;*/

        Ok(ExitStatus::)
    }

    fn exec_script_impl(&mut self,
                        script_path: &Path,
                        args: &[String],
                        cwd: Option<&Path>,
                        run_as: Option<&str>,
                        output: &mut Write,
                        stdout: Stdio,
                        stderr: Stdio) -> Result<ExitStatus, SshError>
    {
        let mut script = match File::open(&script_path) {
            Ok(file) => file,
            Err(err) => {
                return Err(SshError::ScriptOpenError(err).into());
            }
        };

        let mut usr_cmd = CommandBuilder::new(self.config().shell_cmd());
        if let Some(user) = run_as {
            usr_cmd.run_as(self.config().runas_cmd(), user);
        }
        if let Some(cwd) = cwd {
            usr_cmd.cwd(self.config().cd_cmd(), cwd.to_str().unwrap());
        }

        usr_cmd.arg(self.config().shell_cmd());
        usr_cmd.arg("/dev/stdin");
        for a in args {
            usr_cmd.arg(a);
        }

        log_cmd(&usr_cmd.to_string(), output)?;
        log_stdin(&mut script, output)?;

        let mut cmd = usr_cmd.to_command();
        cmd
            .stdout(stdout)
            .stderr(stderr)
            .stdin(Stdio::from(script));

        println!("{:?}", cmd);
        let status = cmd.status()?;

        log_exit_code(status.code(), output)?;

        Ok(status)
    }*/
}
/*
impl CommandExecutor for LocalExecutor {
    fn exec_command(&mut self,
                    _engine: &EngineRef,
                    _runtime: &ActionRuntime,
                    cmd: &str,
                    args: &[String],
                    cwd: Option<&Path>,
                    run_as: Option<&str>,
                    output: &mut Write,
                    stdout: Stdio,
                    stderr: Stdio) -> CommandResult<ActionResult>
    {
        let status = self.exec_command_impl(cmd, args, cwd, run_as, output, stdout, stderr)?;
        Ok(ActionResult::new(Outcome::Empty, status.code(), None))
    }

    fn exec_script(&mut self,
                   engine: &EngineRef,
                   runtime: &ActionRuntime,
                   script_path: &Path,
                   args: &[String],
                   cwd: Option<&Path>,
                   run_as: Option<&str>,
                   output: &mut Write,
                   stdout: Stdio,
                   stderr: Stdio) -> CommandResult<ActionResult>
    {
        let model = engine.write().model_manager_mut().get(runtime.action().model_id())?;
        let model = model.read();

        let proc = {
            let p = runtime.action().processor_path().apply_one(model.root(), model.root());
            match model.get_processor( &p) {
                Some(p) => p,
                None => return Err(CommandError::Undef), //FIXME (jc)
            }
        };

        let script_path = proc.dir().join(script_path);

        let status = self.exec_script_impl(&script_path, args, cwd, run_as, output, stdout, stderr)?;
        Ok(ActionResult::new(Outcome::Empty, status.code(), None))
    }
}

*/
