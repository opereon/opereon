use super::*;

use tokio::process::{Command, Child};
use tokio::io::{BufReader, AsyncRead, AsyncBufRead, AsyncBufReadExt};
use futures::future::try_join;
use std::process::Stdio;

async fn execute(mut command: Command, log: &OutputLog) -> Result<(), std::io::Error> {
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.stdin(Stdio::null());

    let mut child = command.spawn()?;

    let stdout = BufReader::new(child.stdout.take().unwrap());
    let stderr = BufReader::new(child.stderr.take().unwrap());
    drop(child.stdin.take());

    async fn status(child: Child) -> Result<(), std::io::Error> {
        let status = child.await?;
        println!("status: {}", status);
        Ok(())
    }

    async fn stdout_read<R: AsyncRead + Unpin>(s: BufReader<R>) -> Result<(), std::io::Error> {
        let mut stdout = s.lines();
        while let Some(line) = stdout.next_line().await? {
            println!("out: {}", line);
        }
        println!("out: ---");
        Ok(())
    }

    async fn stderr_read<R: AsyncRead + Unpin>(s: BufReader<R>) -> Result<(), std::io::Error> {
        let mut stderr = s.lines();
        while let Some(line) = stderr.next_line().await? {
            println!("err: {}", line);
        }
        println!("err: ---");
        Ok(())
    };

    try_join(stdout_read(stdout), stderr_read(stderr)).await?;

    status(child).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_command() {
        let mut cmd = Command::new("/usr/bin/bash");
        cmd.arg("-c").arg("for i in {1..10}; do echo stdout output; echo stderr output 1>&2; sleep 0.1; done;");

        let log = OutputLog::new();

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async move {
            execute(cmd, &log).await.expect("error");
        });
    }
}