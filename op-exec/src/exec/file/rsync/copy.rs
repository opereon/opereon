use std::io::BufRead;
use std::process::Stdio;
use std::thread::JoinHandle;
use std::time::Duration;

use regex::Regex;

use crate::exec::file::rsync::compare::State;
use crate::RuntimeError;

use super::*;
use crate::exec::file::rsync::RsyncParseErrorDetail::Custom;
use slog::Logger;

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
                    })
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

fn parse_progress<R: BufRead>(mut out: R, operation: OperationRef) -> RsyncParseResult<()> {
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

            operation
                .write()
                .update_progress_step_value(file_idx, loaded_bytes);

            //            eprintln!("File: {} : {}/{}", file_name, loaded_bytes, file_size, );

            if progress_info.len() == 6 {
                //                            eprintln!("file_completed: {:?}", file_name);
                operation.write().update_progress_step_value_done(file_idx);
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
            operation.write().update_progress_step_value_done(file_idx);

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

pub fn rsync_copy(config: &RsyncConfig, params: &RsyncParams) -> RsyncResult<TaskResult> {
    let (stdout, stdout_writer) = pipe().map_err_to_diag()?;
    let (stderr, stderr_writer) = pipe().map_err_to_diag()?;

    let run_stdout = move || {
        let buf = BufReader::new(stdout);

        for line in buf.lines() {
            match line {
                Ok(line) => println!("out: {}", line), // FIXME ws what to do with output?
                Err(err) => return Err(err).map_err_to_diag(),
            }
        }
        Ok(())
    };

    let run_stderr = move || {
        let buf = BufReader::new(stderr);

        for line in buf.lines() {
            match line {
                Ok(line) => println!("err: {}", line), // FIXME ws what to do with output?
                Err(err) => return Err(err).map_err_to_diag(),
            }
        }
        Ok(())
    };

    let hout: JoinHandle<RsyncResult<()>> = std::thread::spawn(run_stdout);
    let herr: JoinHandle<RsyncResult<()>> = std::thread::spawn(run_stderr);
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
            .current_dir(kg_diag::io::fs::current_dir()?)
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_writer))
            .stderr(Stdio::from(stderr_writer))
            .spawn()
            .map_err(RsyncErrorDetail::spawn_err)?
    };
    let status;
    loop {
        if let Some(s) = child.try_wait().map_err(RsyncErrorDetail::spawn_err)? {
            status = Some(s);
            break;
        } else {
            std::thread::sleep(Duration::new(0, 0));
        }
    }

    let status = status.unwrap();

    hout.join().expect("panic while reading stdout")?;
    herr.join().expect("panic while reading stderr")?;

    Ok(TaskResult::new(Outcome::Empty, status.code(), None))
}

#[derive(Debug)]
pub struct FileCopyOperation {
    operation: OperationRef,
    engine: EngineRef,
    curr_dir: PathBuf,
    src_path: PathBuf,
    dst_path: PathBuf,
    chown: Option<String>,
    chmod: Option<String>,
    host: Host,
    status: Arc<Mutex<Option<RsyncResult<ExitStatus>>>>,
    running: bool,
    logger: Logger,
}

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
            .dst_hostname(self.host.ssh_dest().hostname())
            .remote_shell(ssh_session.read().remote_shell_call());
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

        let operation = self.operation.clone();

        let run_stdout = move || {
            let mut buf = BufReader::new(stdout);

            if let Err(err) = parse_progress(&mut buf, operation) {
                // TODO ws report this error somehow?
                println!("Error parsing rsync progress: {}", err)
            };
            Ok(())
        };

        let run_stderr = move || {
            let buf = BufReader::new(stderr);

            for line in buf.lines() {
                match line {
                    Ok(line) => println!("err: {}", line),
                    Err(err) => return Err(err),
                }
            }
            Ok(())
        };

        let _hout: JoinHandle<std::io::Result<()>> = std::thread::spawn(run_stdout);
        let _herr: JoinHandle<std::io::Result<()>> = std::thread::spawn(run_stderr);
        Ok((stdout_writer, stderr_writer))
    }

    fn start_copying(&mut self) -> RsyncResult<()> {
        let params = self.prepare_params()?;
        let config = self.engine.read().config().exec().file().rsync().clone();
        let (stdout, stderr) = self.spawn_std_watchers()?;

        let status = self.status.clone();
        let operation = self.operation.clone();

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

                //                eprintln!("command = {:?}", command);

                let mut child = command.spawn().map_err(RsyncErrorDetail::spawn_err)?;
                let res = child.wait().map_err(RsyncErrorDetail::spawn_err)?;
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

    fn calculate_progress(&mut self) -> RsyncResult<()> {
        let mut executor = create_file_executor(&self.host, &self.engine)?;

        let result = executor.file_compare(
            &self.engine,
            &self.curr_dir,
            &self.src_path,
            &self.dst_path,
            self.chown.as_ref().map(|s| s.as_ref()),
            self.chmod.as_ref().map(|s| s.as_ref()),
            false,
        )?;

        let mut progresses = vec![];

        for diff in result.diffs() {
            match diff.state() {
                State::Missing | State::Modified(_) => {
                    let file_max = diff.file_size() as f64;
                    progresses.push(Progress::with_file_name(
                        0.,
                        file_max,
                        Unit::Bytes,
                        diff.file_path().to_string_lossy().into(),
                    ))
                }
                _ => {}
            }
        }

        let total_progress = Progress::from_steps(progresses);

        self.operation.write().set_progress(total_progress);

        Ok(())
    }

    pub fn status(&self) -> MutexGuard<Option<RsyncResult<ExitStatus>>> {
        self.status.lock().unwrap()
    }
}

impl Future for FileCopyOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if !self.running {
            self.start_copying()?;

            self.running = true;
            return Ok(Async::NotReady);
        }

        match self.status().take() {
            Some(Ok(status)) => {
                if status.success() {
                    Ok(Async::Ready(Outcome::Empty))
                } else {
                    Err(RsyncErrorDetail::RsyncProcessStatus { status }).into_diag_res()
                }
            }
            Some(Err(err)) => Err(err),
            None => Ok(Async::NotReady),
        }
    }
}

impl OperationImpl for FileCopyOperation {
    fn init(&mut self) -> RuntimeResult<()> {
        // FIXME blocking call - implement as future
        self.calculate_progress()?;
        Ok(())
    }
}

/*
pub type Loaded = u64;
pub type FileSize = Loaded;
pub type FileName = String;

pub type Handler<T> = Arc<Mutex<Option<Box<T>>>>;


pub struct RsyncSend {
    on_complete: Handler<Fn(Meta) + Send + 'static>,
    on_progress: Handler<Fn(Loaded, FileSize, FileName) + Send + 'static>,
    on_file_begin: Handler<Fn(FileName, FileSize) + Send + 'static>,
    on_file_complete: Handler<Fn(FileName) + Send + 'static>,
    on_parse_error: Handler<Fn() + Send + 'static>,
}



impl RsyncSend {
    pub fn new() -> RsyncSend {
        RsyncSend {
            on_complete: Arc::new(Mutex::new(None)),
            on_progress: Arc::new(Mutex::new(None)),
            on_file_begin: Arc::new(Mutex::new(None)),
            on_file_complete: Arc::new(Mutex::new(None)),
            on_parse_error: Arc::new(Mutex::new(None)),
        }
    }

    *//*
    pub fn send(self, host: &Host, cfg: Config) -> io::Result<JoinHandle<RsyncResult<Meta>>> {
        let host = host.clone();

        let rsync = self.clone();

        let cfg = Arc::new(cfg);

        let cfg_cl = cfg.clone();
        Builder::new()
            .name("rsyc_send".to_string())
            .spawn(move || {
                let (stdout, stdout_writer) = pipe()?;
                let (stderr, stderr_writer) = pipe()?;

                let session = sessions.lock().get(&host, &auth)?;

                let mut rsync_cmd = Command::new(RSYNC_CMD);

                let ssh_cmd = session.read().ssh_cmd_string();

                // build rsync command
                rsync_cmd
                    .arg("--progress")
                    .arg("--super") // fail on permission denied
                    .arg("--recursive")
                    .arg("--links") // copy symlinks as symlinks
                    .arg("--times") // preserve modification times
                    .arg("--out-format=[%n][%l]")
                    .args(&["--rsh", &ssh_cmd]); // custom ssh command for reuse existing ssh connection

                configure_perms(&mut rsync_cmd, &cfg);

                configure_paths(&mut rsync_cmd, &cfg);

                //configure stdout and stderr
                rsync_cmd.stdout(stdout_writer.into_stdio());
                rsync_cmd.stderr(stderr_writer.into_stdio());

                //spawn progress notification thread

                let rsync_cl = rsync.clone();
                let sent_files = Arc::new(Mutex::new(vec![]));
                let sent = sent_files.clone();
                Builder::new()
                    .name("rsyc_progress".to_string())
                    .spawn(move || {
                        match handle_progress(rsync_cl.clone(), stdout, sent) {
                            Err(err) => {
                                rsync_cl.trigger_on_parse_error();
                                eprintln!("Rsync output parse multi.rs{:?}", err);
                            }
                            _ => {}
                        };
                    })?;

//                eprintln!("rsync_cmd = {:#?}", &rsync_cmd);
                let exit_status = rsync_cmd.spawn()?.wait()?;
                drop(session);

                drop(rsync_cmd); // Important because of deadlock!

                let stderr = io::BufReader::new(stderr);

                let err_lines = stderr
                    .lines()
                    .filter_map(|res| {
                        match res {
                            Ok(line) => Some(line),
                            Err(err) => {
                                eprintln!("Error in stderr line = {:?}", err);
                                None
                            }
                        }
                    })
                    .collect::<Vec<String>>();

                if exit_status.code().is_none() {
                    return Err(RsyncError::RsyncProcessTerminated);
                }
                let mut sent = sent_files.lock().unwrap();

                let meta = Meta {
                    success: exit_status.success(),
                    chmod: cfg_cl.chmod.clone(),
                    chown: cfg_cl.chown.clone(),
                    exit_code: exit_status.code().unwrap(),
                    sent_files: sent.drain(..).collect(),
                    stderr: err_lines,
                };
                drop(sent);

                rsync.trigger_on_complete(meta.clone());
                rsync.cleanup();
                Ok(meta)
            })
    }*//*

    fn cleanup(&self) {
        *self.on_parse_error.lock().unwrap() = None;
        *self.on_file_complete.lock().unwrap() = None;
        *self.on_file_begin.lock().unwrap() = None;
        *self.on_complete.lock().unwrap() = None;
        *self.on_progress.lock().unwrap() = None;
    }

    pub fn on_complete<F>(&self, f: F)
        where F: Fn(Meta) + Send + 'static
    {
        let mut data = self.on_complete.lock().unwrap();
        *data = Some(Box::new(f));
    }

    pub fn on_progress<F>(&self, f: F)
        where F: Fn(Loaded, FileSize, FileName) + Send + 'static
    {
        let mut data = self.on_progress.lock().unwrap();
        *data = Some(Box::new(f));
    }

    pub fn on_file_begin<F>(&self, f: F)
        where F: Fn(FileName, FileSize) + Send + 'static
    {
        let mut data = self.on_file_begin.lock().unwrap();
        *data = Some(Box::new(f));
    }

    pub fn on_file_complete<F>(&self, f: F)
        where F: Fn(FileName) + Send + 'static
    {
        let mut data = self.on_file_complete.lock().unwrap();
        *data = Some(Box::new(f));
    }

    /// Set callback for rsync output parse multi.rs.
    /// This multi.rs means, that rsync progress output cannot be parsed properly.
    /// After parse multi.rs occurs only 'on_complete' will be triggered.
    /// Possible cause of this multi.rs may be rsync binary version other than 3.1.3
    pub fn on_parse_error<F>(&self, f: F)
        where F: Fn() + Send + 'static
    {
        let mut data = self.on_parse_error.lock().unwrap();
        *data = Some(Box::new(f));
    }
    fn trigger_on_complete(&self, meta: Meta) {
        let cb = self.on_complete.lock().unwrap();
        if let Some(ref f) = *cb {
            f(meta);
        }
    }

    fn trigger_on_progress(&self, loaded: Loaded, file_size: FileSize, file_name: FileName) {
        let cb = self.on_progress.lock().unwrap();
        if let Some(ref f) = *cb {
            f(loaded, file_size, file_name);
        }
    }

    fn trigger_on_file_complete(&self, file_name: FileName) {
        let cb = self.on_file_complete.lock().unwrap();
        if let Some(ref f) = *cb {
            f(file_name);
        }
    }

    fn trigger_on_file_begin(&self, file_name: FileName, file_size: FileSize) {
        let cb = self.on_file_begin.lock().unwrap();
        if let Some(ref f) = *cb {
            f(file_name, file_size);
        }
    }

    fn trigger_on_parse_error(&self) {
        let cb = self.on_parse_error.lock().unwrap();
        if let Some(ref f) = *cb {
            f();
        }
    }
    fn clone(&self) -> RsyncSend {
        RsyncSend {
            on_complete: self.on_complete.clone(),
            on_progress: self.on_progress.clone(),
            on_file_begin: self.on_file_begin.clone(),
            on_file_complete: self.on_file_complete.clone(),
            on_parse_error: self.on_parse_error.clone(),
        }
    }
}

fn handle_progress(rsync: RsyncSend, stdout: PipeReader, sent_files: Arc<Mutex<Vec<String>>>) -> Result<(), ParseError> {
    let stdout = std::io::BufReader::new(stdout);

    let mut file_size: u64 = 0;
    let mut file_name: String = String::new();
    let mut file_completed = true;

    let lines = stdout.lines()
        .skip(1); // skip first line: "sending incremental file list"

    let line_endings_reg = Regex::new(r"\n\r|\r|\n").unwrap();
    let file_reg = Regex::new(r"[\[\]]").unwrap();
    let progress_reg = Regex::new(r"[ ]").unwrap();

    'outer: for res in lines {
        match res {
            Ok(line) => {
                let lines = line_endings_reg.split(&line)
                    .filter(|s| !s.is_empty());

                'inner: for line in lines {
                    if !file_completed {
                        let progress_info = progress_reg.split(line)
                            .filter(|s| !s.is_empty())
                            .collect::<Vec<&str>>();

                        check_progress_info(&progress_info)?;

                        let loaded_bytes = progress_info[0].replace(",", "");
                        let loaded_bytes = loaded_bytes.parse::<Loaded>();

                        if loaded_bytes.is_err() {
                            return Err(ParseError { line: line!() });
                        }
                        let loaded_bytes = loaded_bytes.unwrap();

                        rsync.trigger_on_progress(loaded_bytes, file_size, file_name.clone());

                        if progress_info.len() == 6 {
                            sent_files.lock().unwrap().push(file_name.clone());
                            rsync.trigger_on_file_complete(file_name.clone());

                            file_completed = true;
                        }
                        continue 'inner;
                    }

                    let file_info = file_reg.split(line)
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<&str>>();

                    check_file_info(&file_info)?;

                    let res = file_info[1].parse::<FileSize>();
                    if res.is_err() {
                        return Err(ParseError { line: line!() });
                    }

                    file_name = file_info[0].to_string();
                    file_size = res.unwrap();


                    if file_name.ends_with("/") { // no need to notify about directories processing
                        file_completed = true;
                        continue 'inner;
                    }
                    file_completed = false;
                    rsync.trigger_on_file_begin(file_name.clone(), file_size);
                }
            }
            Err(err) => {
                eprintln!("err = {:?}", err);
                return Err(ParseError { line: line!() });
            }
        }
    }
    Ok(())
}


#[inline(always)]
fn check_progress_info(progress_info: &Vec<&str>) -> Result<(), ParseError> {
    if progress_info.len() == 4 || progress_info.len() == 6 {
        return Ok(());
    }
    Err(ParseError { line: line!() })
}


#[inline(always)]
fn check_file_info(file_info: &Vec<&str>) -> Result<(), ParseError> {
    if file_info.len() != 2 {
        return Err(ParseError { line: line!() });
    }
    Ok(())
}
*/
