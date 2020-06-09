use std::io::BufRead;
use std::process::Stdio;
use std::thread::JoinHandle;
use std::time::Duration;

use regex::Regex;

use super::compare::State;
use tokio::io::{BufReader, AsyncBufReadExt, AsyncRead};
use super::*;
use slog::Logger;
use std::sync::{MutexGuard, Arc, Mutex};
use os_pipe::pipe;
use crate::rsync::RsyncParseErrorDetail::Custom;
use futures::future::try_join;

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

fn parse_progress<R: BufRead>(mut out: R) -> RsyncParseResult<()> {
    let mut file_name: String;
    let mut file_completed = true;
    let mut file_idx = 0;

    let file_reg = Regex::new(r"[\[\]]").unwrap();
    let progress_reg = Regex::new(r"[ ]").unwrap();

    let mut buf = Vec::new();

    let delimiter = |b| b == b'\n' || b == b'\r';

    // skip first line: "sending incremental file list"
    read_until(&mut out, delimiter, &mut buf)?;
    buf.clear();

    while read_until(&mut out, delimiter, &mut buf)? != 0 {
        let line = String::from_utf8_lossy(buf.as_slice());
        // skip parsing when line is empty
        if line == "\n" || line == "\r" || line.is_empty() {
            buf.clear();
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

            // operation
            //     .write()
            //     .update_progress_step_value(file_idx, loaded_bytes);

            //            eprintln!("File: {} : {}/{}", file_name, loaded_bytes, file_size, );

            if progress_info.len() == 6 {
                //                            eprintln!("file_completed: {:?}", file_name);
                // operation.write().update_progress_step_value_done(file_idx);
                file_idx += 1;
                file_completed = true;
            }
            buf.clear();
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
            buf.clear();
            continue;
        }
        file_completed = false;
        buf.clear()
    }
    Ok(())
}

async fn rsync_copy(
    config: &RsyncConfig,
    params: &RsyncParams,
    log: &OutputLog,
) -> RsyncResult<()> {
    // let out_log = log.clone();
    // let err_log = log.clone();
    // let run_stdout = move || {
    //     let buf = BufReader::new(stdout);
    //
    //     for line in buf.lines() {
    //         match line {
    //             Ok(line) => out_log.log_stdout(line.as_bytes())?,
    //             Err(err) => return Err(err).map_err_to_diag(),
    //         }
    //     }
    //     Ok(())
    // };
    //
    // let run_stderr = move || {
    //     let buf = BufReader::new(stderr);
    //
    //     for line in buf.lines() {
    //         match line {
    //             Ok(line) => err_log.log_stderr(line.as_bytes())?,
    //             Err(err) => return Err(err).map_err_to_diag(),
    //         }
    //     }
    //     Ok(())
    // };
    //
    // let hout: JoinHandle<RsyncResult<()>> = std::thread::spawn(run_stdout);
    // let herr: JoinHandle<RsyncResult<()>> = std::thread::spawn(run_stderr);
    let mut child = {
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
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // log.log_cmd(&format!("{:?}", rsync_cmd))?;
        rsync_cmd.spawn().map_err(RsyncErrorDetail::spawn_err)?
    };
    let stdout = BufReader::new(child.stdout.take().unwrap());
    let stderr = BufReader::new(child.stderr.take().unwrap());
    drop(child.stdin.take());

    async fn stdout_read<R: AsyncRead + Unpin>(s: BufReader<R>) -> Result<(), std::io::Error> {
        let mut stdout = s.lines();
        while let Some(line) = stdout.next_line().await? {
            println!("out: {:?}", line);
        }
        println!("out: ---");
        Ok(())
    }

    async fn stderr_read<R: AsyncRead + Unpin>(s: BufReader<R>) -> Result<(), std::io::Error> {
        let mut stderr = s.lines();
        while let Some(line) = stderr.next_line().await? {
            println!("err: {:?}", line);
        }
        println!("err: ---");
        Ok(())
    }
    ;

    try_join(stdout_read(stdout), stderr_read(stderr)).await.map_err_to_diag()?;

    let status = child.await.map_err_to_diag()?;
    log.log_status(status.code())?;

    // hout.join().expect("panic while reading stdout")?;
    // herr.join().expect("panic while reading stderr")?;

    Ok(())
}

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
        let mut params = RsyncParams::new("./", "./test_file.txt", "./target_dir/");
        let log = OutputLog::new();

        let mut rt = tokio::runtime::Runtime::new().expect("runtime");

        rt.block_on(async move {
            rsync_copy(&cfg, &params, &log).await.expect("error");
            println!("{}", log)
        });
    }
}