use std::io::{BufRead, BufReader, Read};
use std::process::Stdio;

use regex::Regex;

use super::*;
use crate::rsync::RsyncParseErrorDetail::Custom;

use crate::utils::spawn_blocking;
use os_pipe::pipe;
use shared_child::SharedChild;
use std::borrow::Cow;
use std::sync::Arc;
use std::thread;
use tokio::sync::{mpsc, oneshot};

type Loaded = u64;

#[inline(always)]
fn check_progress_info(progress_info: &[&str]) -> RsyncParseResult<()> {
    if progress_info.len() == 4 || progress_info.len() == 6 {
        return Ok(());
    }
    RsyncParseErrorDetail::custom_line(line!())
}

#[inline(always)]
fn check_file_info(file_info: &[&str]) -> RsyncParseResult<()> {
    if file_info.len() != 2 {
        return RsyncParseErrorDetail::custom_line(line!());
    }
    Ok(())
}

fn read_until<R: BufRead + ?Sized>(
    r: &mut R,
    pred: impl Fn(u8) -> bool,
    buf: &mut Vec<u8>,
) -> RsyncParseResult<usize> {
    let mut read = 0;
    loop {
        let (done, used) = {
            let available = match r.fill_buf() {
                Ok(n) => n,
                Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => {
                    return Err(e).map_err_to_diag().map_err_as_cause(|| Custom {
                        line: line!(),
                        output: String::new(),
                    });
                }
            };

            let mut found = None;

            for (i, item) in available.iter().enumerate() {
                if pred(*item) {
                    found = Some(i);
                    break;
                }
            }

            match found {
                Some(i) => {
                    buf.extend_from_slice(&available[..=i]);
                    (true, i + 1)
                }
                None => {
                    buf.extend_from_slice(available);
                    (false, available.len())
                }
            }
        };
        r.consume(used);
        read += used;
        if done || used == 0 {
            return Ok(read);
        }
    }
}

#[derive(Debug)]
pub struct ProgressInfo {
    pub file_name: String,
    pub loaded_bytes: f64,
    pub is_completed: bool,
}

impl ProgressInfo {
    pub fn new(file_name: String, loaded_bytes: f64, is_completed: bool) -> Self {
        ProgressInfo {
            file_name,
            loaded_bytes,
            is_completed,
        }
    }
}

pub struct ProgressParser<R: Read> {
    reader: BufReader<R>,
    buf: Vec<u8>,
    progress_sender: mpsc::UnboundedSender<ProgressInfo>,
}

fn newline_delimiter(b: u8) -> bool {
    b == b'\n' || b == b'\r'
}

impl<R: Read> ProgressParser<R> {
    pub fn new(
        reader: R,
        progress_sender: mpsc::UnboundedSender<ProgressInfo>,
    ) -> ProgressParser<R> {
        ProgressParser {
            reader: BufReader::new(reader),
            buf: Vec::new(),
            progress_sender,
        }
    }

    pub fn next_line(&mut self) -> RsyncParseResult<Option<Cow<str>>> {
        self.buf.clear();
        let read = read_until(&mut self.reader, newline_delimiter, &mut self.buf)?;
        if read == 0 {
            Ok(None)
        } else {
            let line = String::from_utf8_lossy(self.buf.as_slice());
            Ok(Some(line))
        }
    }

    pub fn parse_progress(&mut self) -> RsyncParseResult<()> {
        let mut file_name: String = String::new();
        let mut file_completed = true;
        // let mut file_idx: i32 = 0;

        let file_reg = Regex::new(r"[\[\]]").unwrap();
        let progress_reg = Regex::new(r"[ ]").unwrap();

        // skip first line: "sending incremental file list"
        self.next_line()?;

        while let Some(line) = self.next_line()? {
            // eprintln!("line = {:?}", line);
            // skip parsing when line is empty
            if line == "\n" || line == "\r" || line.is_empty() {
                continue;
            }
            // skip \n or \r at the end of line
            let line = &line[..line.len() - 1];

            if !file_completed && !line.starts_with('[') {
                let progress_info = progress_reg
                    .split(&line)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<&str>>();

                check_progress_info(&progress_info)?;

                let loaded_bytes = progress_info[0].replace(",", "");
                let loaded_bytes = loaded_bytes.parse::<Loaded>();

                if loaded_bytes.is_err() {
                    return RsyncParseErrorDetail::custom_line(line!());
                }
                let loaded_bytes = loaded_bytes.unwrap() as f64;

                if progress_info.len() == 6 {
                    let _ = self.progress_sender.send(ProgressInfo::new(
                        file_name.clone(),
                        loaded_bytes,
                        true,
                    ));
                    // eprintln!("File completed: {:?}", file_name);
                    // file_idx += 1;
                    file_completed = true;
                } else {
                    // eprintln!("File: {} : {}", file_name, loaded_bytes);
                    let _ = self.progress_sender.send(ProgressInfo::new(
                        file_name.clone(),
                        loaded_bytes,
                        false,
                    ));
                }
                continue;
            }

            let file_info = file_reg
                .split(&line)
                .filter(|s| !s.is_empty())
                .collect::<Vec<&str>>();

            check_file_info(&file_info)?;

            let res = file_info[1].parse::<FileSize>();
            if res.is_err() {
                return RsyncParseErrorDetail::custom_line(line!());
            }

            file_name = file_info[0].to_string();

            if file_name.ends_with('/') || file_name.ends_with("/.") {
                // directory - no progress value
                // operation.write().update_progress_step_value_done(file_idx);

                file_completed = true;
                // file_idx += 1;
                continue;
            }
            file_completed = false;
        }
        Ok(())
    }

    pub fn into_inner(self) -> R {
        self.reader.into_inner()
    }
}

pub struct RsyncCopy {
    child: Arc<SharedChild>,
    done_rx: oneshot::Receiver<Result<ExitStatus, std::io::Error>>,
    log: OutputLog,
}

impl RsyncCopy {
    pub fn spawn(
        config: &RsyncConfig,
        params: &RsyncParams,
        progress_sender: mpsc::UnboundedSender<ProgressInfo>,
        log: &OutputLog,
    ) -> RsyncResult<RsyncCopy> {
        let (out_reader, out_writer) = pipe().unwrap();
        let (err_reader, err_writer) = pipe().unwrap();

        let child = {
            let mut rsync_cmd = params.to_cmd(config);
            rsync_cmd
                .arg("--progress")
                .arg("--super") // fail on permission denied
                .arg("--recursive")
                .arg("--links") // copy symlinks as symlinks
                .arg("--times") // preserve modification times
                .arg("--out-format=[%f][%l]") // log format described in https://download.samba.org/pub/rsync/rsyncd.conf.html
                .env("TERM", "xterm-256color")
                .stdin(Stdio::null())
                .stdout(out_writer)
                .stderr(err_writer);
            log.log_in(format!("{:?}", rsync_cmd).as_bytes())?;
            let child = SharedChild::spawn(&mut rsync_cmd).map_err(RsyncErrorDetail::spawn_err)?;
            Arc::new(child)
        };

        let l = log.clone();
        thread::spawn(move || {
            let mut parser = ProgressParser::new(out_reader, progress_sender);

            if let Err(err) = parser.parse_progress() {
                // TODO ws log error
                // TODO ws each line should be logged to OutputLog
                eprintln!("Error parsing rsync progress = {}", err);

                // in case of parsing error drain stdout to prevent main process hang/failure
                let stdout = parser.into_inner();
                l.consume_stdout(stdout).expect("Error logging stdout");
            };
        });

        let l = log.clone();
        thread::spawn(move || {
            //drain stderr to prevent main process hang/failure
            l.consume_stderr(err_reader).expect("Error logging stderr")
        });

        let c = child.clone();
        let done_rx = spawn_blocking(move || c.wait());

        Ok(RsyncCopy {
            done_rx,
            child,
            log: log.clone(),
        })
    }

    pub async fn wait(self) -> RsyncResult<()> {
        let status = self
            .done_rx
            .await
            .unwrap()
            .map_err(RsyncErrorDetail::spawn_err)?;

        self.log.log_status(status.code())?;

        match status.code() {
            None => Err(RsyncErrorDetail::RsyncTerminated.into()),
            Some(0) => Ok(()),
            Some(_c) => RsyncErrorDetail::process_status(status),
        }
    }

    pub fn child(&self) -> &Arc<SharedChild> {
        &self.child
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsync_copy_test() {
        let cfg = RsyncConfig::default();
        let params = RsyncParams::new("./", "./../target/debug/incremental", "./../target/debug2");
        let log = OutputLog::new();

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async move {
            let (tx, mut rx) = mpsc::unbounded_channel();
            tokio::spawn(async move {
                while let Some(progress) = rx.recv().await {
                    eprintln!("progress = {:?}", progress);
                }
            });

            let copy = RsyncCopy::spawn(&cfg, &params, tx, &log).expect("error");

            let _res = copy.wait().await.expect("Error");
            println!("{}", log)
        });
    }
}
