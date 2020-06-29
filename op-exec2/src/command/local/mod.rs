use super::*;
use std::process::ExitStatus;

mod config;

use config::LocalConfig;
use os_pipe::pipe;
use std::io::Write;

async fn run_command(
    cmd: &str,
    args: &[String],
    env: Option<&EnvVars>,
    cwd: Option<&Path>,
    run_as: Option<&str>,
    config: &LocalConfig,
    log: &OutputLog,
) -> CommandResult<ExitStatus> {
    let mut builder = prepare_builder(cmd, env, run_as, config);

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

    log.log_in(format!("{:?}", command).as_bytes())?;
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

async fn run_script(
    script: SourceRef<'_>,
    args: &[String],
    env: Option<&EnvVars>,
    cwd: Option<&Path>,
    run_as: Option<&str>,
    config: &LocalConfig,
    log: &OutputLog,
) -> CommandResult<ExitStatus> {
    let mut builder = prepare_builder(config.shell_cmd(), env, run_as, config);

    match script {
        SourceRef::Path(path) => {
            builder.arg(path.to_string_lossy());
        }
        SourceRef::Source(_) => {
            builder.arg("/dev/stdin");
        }
    }

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

    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    if let SourceRef::Source(src) = script {
        let (r_in, mut w_in) = pipe().unwrap();
        command.stdin(Stdio::from(r_in));
        w_in.write_all(src.as_bytes()).map_err_to_diag()?;
    } else {
        command.stdin(Stdio::null());
    }

    log.log_in(format!("{:?}", command).as_bytes())?;
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

fn prepare_builder(
    cmd: &str,
    env: Option<&LinkedHashMap<String, String>>,
    run_as: Option<&str>,
    config: &LocalConfig,
) -> CommandBuilder {
    let builder = if let Some(user) = run_as {
        // TODO ws is this implementation ok? Will work only for `runas_cmd = sudo`
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
    builder
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    #[test]
    fn run_script_test() {
        let cfg = LocalConfig::default();

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        let mut env = EnvVars::new();

        env.insert(
            "TEST_ENV_VAR".into(),
            "This is environment variable content ".into(),
        );

        // let script = SourceRef::Path("./example_script.sh".as_ref());

        let script = SourceRef::Source(
            r#"
        echo 'printing cwd'
        pwd

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
        rt.block_on(async move {
            let log = OutputLog::new();

            let status = run_script(
                script,
                &["-a1".into(), "-l2".into()],
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
