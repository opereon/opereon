use super::*;

use tokio::process::{Command, Child};
use tokio::io::{BufReader, AsyncRead, AsyncBufRead, AsyncBufReadExt};
use futures::future::try_join;
use std::process::Stdio;
use std::pin::Pin;
use futures::task::{Context, Poll};
use tokio::future::poll_fn;

#[pin_project]
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
struct Lines<R> {
    #[pin]
    reader: R,
    buf: String,
    bytes: Vec<u8>,
    read: usize,
}

fn lines<R: AsyncBufRead>(reader: R) -> Lines<R> {
    Lines {
        reader,
        buf: String::new(),
        bytes: Vec::new(),
        read: 0,
    }
}

impl<R: AsyncBufRead + Unpin> Lines<R> {
    pub async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        poll_fn(|cx| Pin::new(&mut *self).poll_next_line(cx)).await
    }

    /// Obtain a mutable reference to the underlying reader
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.reader
    }

    /// Obtain a reference to the underlying reader
    pub fn get_ref(&mut self) -> &R {
        &self.reader
    }

    /// Unwraps this `Lines<R>`, returning the underlying reader.
    ///
    /// Note that any leftover data in the internal buffer is lost.
    /// Therefore, a following read from the underlying reader may lead to data loss.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

impl<R: AsyncBufRead> Lines<R> {
    #[doc(hidden)]
    pub fn poll_next_line(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<std::io::Result<Option<String>>> {
        let me = self.project();
        let n = ready!(read_line_internal(me.reader, cx, me.buf, me.bytes, me.read))?;
        if n == 0 && me.buf.is_empty() {
            return Poll::Ready(Ok(None));
        }
        Poll::Ready(Ok(Some(std::mem::replace(me.buf, String::new()))))
    }
}

impl<R: AsyncBufRead> tokio::stream::Stream for Lines<R> {
    type Item = std::io::Result<String>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(match ready!(self.poll_next_line(cx)) {
            Ok(Some(line)) => Some(Ok(line)),
            Ok(None) => None,
            Err(err) => Some(Err(err)),
        })
    }
}

fn read_line_internal<R: AsyncBufRead + ?Sized>(
    reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    buf: &mut String,
    bytes: &mut Vec<u8>,
    read: &mut usize,
) -> Poll<std::io::Result<usize>> {
    let ret = ready!(read_until_internal(reader, cx, bytes, read));
    if std::str::from_utf8(&bytes).is_err() {
        Poll::Ready(ret.and_then(|_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "stream did not contain valid UTF-8",
            ))
        }))
    } else {
        debug_assert!(buf.is_empty());
        debug_assert_eq!(*read, 0);
        // Safety: `bytes` is a valid UTF-8 because `str::from_utf8` returned `Ok`.
        std::mem::swap(unsafe { buf.as_mut_vec() }, bytes);
        Poll::Ready(ret)
    }
}

fn read_until_internal<R: AsyncBufRead + ?Sized>(
    mut reader: Pin<&mut R>,
    cx: &mut Context<'_>,
    buf: &mut Vec<u8>,
    read: &mut usize,
) -> Poll<std::io::Result<usize>> {
    loop {
        let (done, used) = {
            let available = ready!(reader.as_mut().poll_fill_buf(cx))?;
            let mut nl = std::usize::MAX;
            for (i, &b) in available.iter().enumerate() {
                if b == b'\r' || b == b'\n' {
                    nl = i;
                    break;
                }
            }
            if nl < std::usize::MAX {
                buf.extend_from_slice(&available[..=nl]);
                (true, nl + 1)
            } else {
                buf.extend_from_slice(available);
                (false, available.len())
            }
        };
        reader.as_mut().consume(used);
        *read += used;
        if done || used == 0 {
            return Poll::Ready(Ok(std::mem::replace(read, 0)));
        }
    }
}


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
        let mut stdout = lines(s);
        while let Some(line) = stdout.next_line().await? {
            println!("out: {:?}", line);
        }
        println!("out: ---");
        Ok(())
    }

    async fn stderr_read<R: AsyncRead + Unpin>(s: BufReader<R>) -> Result<(), std::io::Error> {
        let mut stderr = lines(s);
        while let Some(line) = stderr.next_line().await? {
            println!("err: {:?}", line);
        }
        println!("err: ---");
        Ok(())
    };

    try_join(stdout_read(stdout), stderr_read(stderr)).await?;

    status(child).await
}

async fn execute_pty(command: std::process::Command, log: &OutputLog) -> Result<(), std::io::Error> {
    let mut session = rexpect::session::spawn_command(command, None).expect("spawn");
    while let Ok(line) = session.read_line() {
        println!("out: {:?}", line);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_command() {
        let mut cmd = Command::new("/usr/bin/bash");
        cmd.arg("-c").arg("for i in {1..10}; do echo stdout output; echo stderr output 1>&2;  done;");

        let log = OutputLog::new();

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async move {
            execute(cmd, &log).await.expect("error");
        });
    }

    #[test]
    fn aaa_command() {
        let mut cmd = tokio::process::Command::new("/usr/bin/rsync");
        cmd.arg("-av")
            .arg("--stats")
            .arg("--progress")
            .arg("--bwlimit=200k")
            .arg("./../target/debug/incremental")
            .arg("./../target/debug2");

        let log = OutputLog::new();

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async move {
            execute(cmd, &log).await.expect("error");
        });
    }
}