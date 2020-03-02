use super::*;

use tokio::process::Command;
use tokio::io::{BufReader, AsyncBufReadExt};

use std::process::Stdio;

async fn execute(mut command: Command, log: &OutputLog) -> Result<(), std::io::Error> {
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(Stdio::null());

    let mut child = command.spawn()?;

    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();
    let mut stderr = BufReader::new(child.stderr.take().unwrap()).lines();
    drop(child.stdin.take());

    tokio::spawn(async move {
        while let Ok(Some(line)) = stdout.next_line().await {
            println!("out: {}", line);
        }
        println!("out: ---");
    });

    tokio::spawn(async move {
        while let Ok(Some(line)) = stderr.next_line().await {
            println!("err: {}", line);
        }
        println!("err: ---");
    });

    let status = child.await?;

    println!("status: {}", status);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_command() {
        let mut cmd = Command::new("/usr/bin/cat");
        cmd.arg("/etc/hosts");

        let log = OutputLog::new();

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async move {
            execute(cmd, &log).await.expect("error");
        });
    }
}