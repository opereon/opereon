use super::*;
use std::process::ExitStatus;

mod config;

use config::LocalConfig;

async fn run_command(
    cmd: &str,
    args: &[String],
    env: Option<&EnvVars>,
    cwd: Option<&Path>,
    run_as: Option<&str>,
    config: &LocalConfig,
    log: &OutputLog,
) -> CommandResult<ExitStatus> {
    let mut builder = if let Some(user) = run_as {
        let mut builder = CommandBuilder::new(config.runas_cmd());
        builder.arg("-u").arg(user);

        if let Some(env) = env {
            let envs = env.keys().map(|s| &**s).collect::<Vec<_>>().join(",");
            builder.arg(format!("--preserve-env={}", envs));
        }

        builder.arg(cmd);
        builder
    } else {
        let cmd = CommandBuilder::new(cmd);
        cmd
    };

    builder.args(args.iter().map(String::as_str));

    if let Some(envs) = env {
        for (k, v) in envs {
            builder.env(k, v);
        }
    }

    let mut command = builder.build();

    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    log.log_in(format!("{:?}", command).as_bytes());
    let mut child = command.spawn().map_err(CommandErrorDetail::spawn_err)?;

    let stdout = BufReader::new(child.stdout.take().unwrap());
    let stderr = BufReader::new(child.stderr.take().unwrap());
    drop(child.stdin.take());

    // TODO ws handle stdout and stderr
    handle_out(stdout, stderr).await?;

    let status = child.await.map_err_to_diag()?;

    log.log_status(status.code())?;

    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_command_test() {
        let cfg = LocalConfig::default();

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        let mut env = EnvVars::new();

        env.insert(
            "TEST_ENV_VAR".into(),
            "This is environment variable content ".into(),
        );

        rt.block_on(async move {
            let log = OutputLog::new();

            let status = run_command(
                "ls",
                &["-a".into(), "-l".into()],
                Some(&env),
                Some(&PathBuf::from("/home")),
                Some("wiktor"),
                &cfg,
                &log,
            )
            .await
            .expect("Error");
            eprintln!("status = {:?}", status);
            eprintln!("log = {}", log);
        });
    }
}
