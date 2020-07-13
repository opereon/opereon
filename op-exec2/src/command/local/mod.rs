use super::*;

pub mod config;

use crate::utils::spawn_blocking;
use config::LocalConfig;
use os_pipe::pipe;
use shared_child::SharedChild;
use std::io::{ Write};
use std::sync::Arc;

pub fn spawn_local_command(
    cmd: &str,
    args: &[String],
    env: Option<&EnvVars>,
    cwd: Option<&Path>,
    run_as: Option<&str>,
    config: &LocalConfig,
    log: &OutputLog,
) -> CommandResult<CommandHandle> {
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
    let (out_reader, out_writer) = pipe().unwrap();
    let (err_reader, err_writer) = pipe().unwrap();
    command
        .stdin(Stdio::null())
        .stdout(out_writer)
        .stderr(err_writer);

    log.log_in(format!("{:?}", command).as_bytes())?;
    let child = SharedChild::spawn(&mut command).map_err(CommandErrorDetail::spawn_err)?;
    drop(command);
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

pub fn spawn_local_script(
    script: SourceRef<'_>,
    args: &[String],
    env: Option<&EnvVars>,
    cwd: Option<&Path>,
    run_as: Option<&str>,
    config: &LocalConfig,
    log: &OutputLog,
) -> CommandResult<CommandHandle> {
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
    let (out_reader, out_writer) = pipe().unwrap();
    let (err_reader, err_writer) = pipe().unwrap();
    command.stdout(out_writer).stderr(err_writer);

    log.log_in(format!("{:?}", command).as_bytes())?;
    if let SourceRef::Source(src) = script {
        let (r_in, mut w_in) = pipe().unwrap();
        command.stdin(Stdio::from(r_in));
        log.log_in(src.as_bytes())?;
        w_in.write_all(src.as_bytes()).map_err_to_diag()?;
    } else {
        command.stdin(Stdio::null());
    }

    let child = SharedChild::spawn(&mut command).map_err(CommandErrorDetail::spawn_err)?;
    drop(command);
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
    use tokio::time::Duration;

    #[test]
    fn cancel_command_test() {
        let cfg = LocalConfig::default();

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        let mut env = EnvVars::new();

        env.insert(
            "TEST_ENV_VAR".into(),
            "This is environment variable content ".into(),
        );

        rt.block_on(async move {
            let log = OutputLog::new();

            let lc = spawn_local_command(
                "yes",
                &[],
                Some(&env),
                Some(&PathBuf::from("/home")),
                None,
                &cfg,
                &log,
            )
            .unwrap();

            let child = lc.child().clone();

            tokio::spawn(async move {
                tokio::time::delay_for(Duration::from_secs(1)).await;
                println!("killing command...");
                child.kill().unwrap();
                println!("signal sent");
            });

            let res = lc.wait().await.unwrap();
            eprintln!("status = {:?}", res);
            assert_eq!(res.code, None);

            // eprintln!("log = {}", log);
        });
    }

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

            let lc = spawn_local_command(
                "ls",
                &["-a".into(), "-l".into()],
                Some(&env),
                Some(&PathBuf::from("/home")),
                Some("wiktor"),
                &cfg,
                &log,
            )
            .expect("Error");

            let res = lc.wait().await.unwrap();

            eprintln!("status = {:?}", res);
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
        rt.block_on(async move {
            let log = OutputLog::new();

            let ls = spawn_local_script(
                script,
                &["-a1".into(), "-l2".into()],
                Some(&env),
                Some(&PathBuf::from("/home")),
                Some("wiktor"),
                &cfg,
                &log,
            )
            .unwrap();

            let out = ls.wait().await.unwrap();
            eprintln!("output = {:?}", out);
            eprintln!("log = {}", log);
        });
    }
}
