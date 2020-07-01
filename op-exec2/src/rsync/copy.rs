use std::io::{BufRead, BufReader, Read};
use std::process::Stdio;

use regex::Regex;

use super::*;
use crate::rsync::RsyncParseErrorDetail::Custom;
use crate::utils::lines;
use futures::future::try_join;

use futures::io::Error;
use os_pipe::pipe;
use shared_child::SharedChild;
use std::borrow::Cow;
use std::sync::Arc;
use std::thread;
use tokio::io::{AsyncBufReadExt, AsyncRead};
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
        let mut file_idx: i32 = 0;

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
                    file_idx += 1;
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
                file_idx += 1;
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
        let (done_tx, done_rx) = oneshot::channel::<Result<ExitStatus, std::io::Error>>();
        let (mut out_reader, out_writer) = pipe().unwrap();
        let (mut err_reader, err_writer) = pipe().unwrap();

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
                eprintln!("Error parsing rsync progress = {}", err);

                // in case of parsing error drain stdout to prevent main process hang/failure
                let mut stdout = parser.into_inner();
                l.consume_stdout(stdout).expect("Error logging stdout");
            };
        });

        let l = log.clone();
        thread::spawn(move || {
            //drain stderr to prevent main process hang/failure
            l.consume_stderr(err_reader).expect("Error logging stderr")
        });

        let c = child.clone();
        thread::spawn(move || {
            let res = c.wait();
            // no receiver means main future was dropped - we can safely skip result
            let _ = done_tx.send(res);
        });

        Ok(RsyncCopy {
            done_rx,
            child,
            log: log.clone(),
        })
    }

    pub async fn wait(self) -> RsyncResult<ExitStatus> {
        let status = self
            .done_rx
            .await
            .unwrap()
            .map_err(RsyncErrorDetail::spawn_err)?;

        Ok(status)
    }

    pub fn child(&self) -> &Arc<SharedChild> {
        &self.child
    }
}

/*
pub async fn rsync_copy(
    config: &RsyncConfig,
    params: &RsyncParams,
    progress_sender: mpsc::UnboundedSender<ProgressInfo>,
    log: &OutputLog,
) -> RsyncResult<ExitStatus> {
    let mut child = {
        let mut rsync_cmd = params.to_cmd(config);
        let mut rsync_cmd = tokio::process::Command::from(rsync_cmd);
        rsync_cmd
            .arg("--progress")
            .arg("--super") // fail on permission denied
            .arg("--recursive")
            .arg("--links") // copy symlinks as symlinks
            .arg("--times") // preserve modification times
            .arg("--out-format=[%f][%l]") // log format described in https://download.samba.org/pub/rsync/rsyncd.conf.html
            .env("TERM", "xterm-256color")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        log.log_in(format!("{:?}", rsync_cmd).as_bytes())?;
        rsync_cmd.spawn().map_err(RsyncErrorDetail::spawn_err)?
    };
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let stderr = BufReader::new(child.stderr.take().unwrap());
    drop(child.stdin.take());

    async fn stdout_read<R: AsyncRead + Unpin>(
        s: BufReader<R>,
        progress_sender: mpsc::UnboundedSender<ProgressInfo>,
    ) -> RsyncResult<()> {
        parse_progress(s, progress_sender).await?;
        Ok(())
    }

    async fn stderr_read<R: AsyncRead + Unpin>(s: BufReader<R>) -> RsyncResult<()> {
        let mut stderr = s.lines();
        while let Some(line) = stderr.next_line().await.map_err_to_diag()? {
            println!("err: {:?}", line);
        }
        println!("err: ---");
        Ok(())
    }

    try_join(stdout_read(stdout, progress_sender), stderr_read(stderr)).await?;

    let status = child.await.map_err_to_diag()?;
    log.log_status(status.code())?;

    Ok(status)
}
*/
/**
impl FileCopyOperation {
    pub fn new(
        operation: OperationRef,
        engine: EngineRef,
        curr_dir: &Path,
        src_path: &Path,
        dst_path: &Path,
        chown: &Option<String>,
        chmod: &Option<String>,
        host: &Host,
    ) -> FileCopyOperation {
        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "curr_dir" => format!("{}", curr_dir.display()),
            "src_path" => format!("{}", src_path.display()),
            "dst_path" => format!("{}", dst_path.display()),
            "host" => format!("{}", host),
        ));

        FileCopyOperation {
            operation,
            engine,
            curr_dir: curr_dir.to_owned(),
            src_path: src_path.to_owned(),
            dst_path: dst_path.to_owned(),
            chown: chown.as_ref().map(|s| s.to_string()),
            chmod: chmod.as_ref().map(|s| s.to_string()),
            host: host.clone(),
            status: Arc::new(Mutex::new(None)),
            running: false,
            logger,
        }
    }

    fn prepare_params(&self) -> RsyncResult<RsyncParams> {
        let ssh_session = self
            .engine
            .write()
            .ssh_session_cache_mut()
            .get(self.host.ssh_dest())?;
        let mut params = RsyncParams::new(&self.curr_dir, &self.src_path, &self.dst_path);
        params
            .dst_username(self.host.ssh_dest().username())
            .dst_hostname(self.host.ssh_dest().hostname())
            .remote_shell(ssh_session.read().remote_shell_cmd());
        if let Some(chown) = &self.chown {
            params.chown(chown.to_owned());
        }
        if let Some(chmod) = &self.chmod {
            params.chmod(chmod.to_owned());
        }
        Ok(params)
    }

    fn spawn_std_watchers(&self) -> RsyncResult<(PipeWriter, PipeWriter)> {
        let (stdout, stdout_writer) = pipe().map_err_to_diag()?;
        let (stderr, stderr_writer) = pipe().map_err_to_diag()?;

        let err_log = self.operation.read().output().clone();
        let logger = self.logger.clone();

        let operation = self.operation.clone();

        let run_stdout = move || {
            let mut buf = BufReader::new(stdout);

            if let Err(err) = parse_progress(&mut buf, operation) {
                warn!(logger, "cannot parse rsync progress! {}", err; "verbosity" => 0);
            };
            Ok(())
        };

        let run_stderr = move || {
            let buf = BufReader::new(stderr);

            for line in buf.lines() {
                match line {
                    Ok(line) => err_log.log_stdout(line.as_bytes())?,
                    Err(err) => return Err(err).map_err_to_diag(),
                }
            }
            Ok(())
        };

        let _hout: JoinHandle<RsyncResult<()>> = std::thread::spawn(run_stdout);
        let _herr: JoinHandle<RsyncResult<()>> = std::thread::spawn(run_stderr);
        Ok((stdout_writer, stderr_writer))
    }

    fn start_copying(&mut self) -> RsyncResult<()> {
        let params = self.prepare_params()?;
        let config = self.engine.read().config().exec().file().rsync().clone();
        let (stdout, stderr) = self.spawn_std_watchers()?;

        let status = self.status.clone();
        let operation = self.operation.clone();

        let output_log = operation.read().output().clone();

        std::thread::spawn(move || {
            let execute_cmd = move || -> RsyncResult<ExitStatus> {
                let mut command = params.to_cmd(&config);
                command
                    .arg("--progress")
                    .arg("--super") // fail on permission denied
                    .arg("--recursive")
                    .arg("--links") // copy symlinks as symlinks
                    .arg("--times") // preserve modification times
                    .arg("--out-format=[%f][%l]")
                    .env("TERM", "xterm-256color")
                    .stdin(Stdio::null())
                    .stdout(Stdio::from(stdout))
                    .stderr(Stdio::from(stderr));

                output_log.log_cmd(&format!("{:?}", command))?;

                let mut child = command.spawn().map_err(RsyncErrorDetail::spawn_err)?;
                let res = child.wait().map_err(RsyncErrorDetail::spawn_err)?;
                output_log.log_status(res.code())?;
                Ok(res)
            };

            match execute_cmd() {
                Ok(stat) => *status.lock().unwrap() = Some(Ok(stat)),
                Err(err) => *status.lock().unwrap() = Some(Err(err)),
            }
            operation.write().notify()
        });
        Ok(())
    }

    // fn calculate_progress(&mut self) -> RsyncResult<()> {
    //     let mut executor = create_file_executor(&self.host, &self.engine)?;
    //
    //     let result = executor.file_compare(
    //         &self.engine,
    //         &self.curr_dir,
    //         &self.src_path,
    //         &self.dst_path,
    //         self.chown.as_ref().map(|s| s.as_ref()),
    //         self.chmod.as_ref().map(|s| s.as_ref()),
    //         false,
    //         self.operation.read().output(),
    //     )?;
    //
    //     let mut progresses = vec![];
    //
    //     for diff in result.diffs() {
    //         match diff.state() {
    //             State::Missing | State::Modified(_) => {
    //                 let file_max = diff.file_size() as f64;
    //                 progresses.push(Progress::with_file_name(
    //                     0.,
    //                     file_max,
    //                     Unit::Bytes,
    //                     diff.file_path().to_string_lossy().into(),
    //                 ))
    //             }
    //             _ => {}
    //         }
    //     }
    //
    //     let total_progress = Progress::from_steps(progresses);
    //
    //     self.operation.write().set_progress(total_progress);
    //
    //     Ok(())
    // }

    pub fn status(&self) -> MutexGuard<Option<RsyncResult<ExitStatus>>> {
        self.status.lock().unwrap()
    }
}\
*/
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rsync_copy_test() {
        let cfg = RsyncConfig::default();
        let mut params =
            RsyncParams::new("./", "./../target/debug/incremental", "./../target/debug2");
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

            let res = copy.wait().await.expect("Error");
            println!("{}", log)
        });
    }
}
