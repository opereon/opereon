use super::*;

use std::io::BufRead;
use std::thread::JoinHandle;
use std::process::Stdio;
use std::time::Duration;
use regex::Regex;

type Loaded = u64;
type FileSize = u64;

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

fn parse_progress<R: BufRead>(mut out: R) -> Result<(), ParseError> {
    let mut file_size: u64 = 0;
    let mut file_name: String = String::new();
    let mut file_completed = true;

    let lines = out.lines()
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
                    if !file_completed && !line.starts_with("["){
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

                        eprintln!("File: {} : {}/{}", file_name, loaded_bytes, file_size, );

//                        rsync.trigger_on_progress(loaded_bytes, file_size, file_name.clone());

                        if progress_info.len() == 6 {
//                            sent_files.lock().unwrap().push(file_name.clone());
//                            rsync.trigger_on_file_complete(file_name.clone());
                            eprintln!("file_completed: {:?}", file_name);

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


                    if file_name.ends_with("/") || file_name.ends_with("/."){ // no need to notify about directories processing
                        file_completed = true;
                        continue 'inner;
                    }
                    file_completed = false;
//                    rsync.trigger_on_file_begin(file_name.clone(), file_size);
                }
            }
            Err(err) => {
                return Err(ParseError { line: line!() });
            }
        }
    }
    Ok(())
}


pub fn rsync_copy(config: &RsyncConfig, params: &RsyncParams) -> Result<TaskResult, RsyncError> {
    let (stdout, stdout_writer) = pipe()?;
    let (stderr, stderr_writer) = pipe()?;

    let run_stdout = move || {
        let buf = BufReader::new(stdout);

        for line in buf.lines() {
            match line {
                Ok(line) => println!("out: {}", line),
                Err(err) => return Err(err),
            }
        }
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

    let hout: JoinHandle<std::io::Result<()>> = std::thread::spawn(run_stdout);
    let herr: JoinHandle<std::io::Result<()>> = std::thread::spawn(run_stderr);
    let mut child = {
        let mut rsync_cmd = params.to_cmd(config);
        rsync_cmd
            .arg("--progress")
            .arg("--super") // fail on permission denied
            .arg("--recursive")
            .arg("--links") // copy symlinks as symlinks
            .arg("--times") // preserve modification times
            .arg("--out-format=[%f][%l]")
            .env("TERM", "xterm-256color")
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout_writer))
            .stderr(Stdio::from(stderr_writer))
            .spawn()?
    };

    let mut status = None;
    loop {
        if let Some(s) = child.try_wait()? {
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
    bin_id: Uuid,
    curr_dir: PathBuf,
    src_path: PathBuf,
    dst_path: PathBuf,
    chown: Option<String>,
    chmod: Option<String>,
    host: Host,
    status: Arc<Mutex<Option<Result<ExitStatus, RuntimeError>>>>,
    running: bool
}

impl FileCopyOperation {
    pub fn new(operation: OperationRef,
               engine: EngineRef,
               bin_id: Uuid,
               curr_dir: &Path,
               src_path: &Path,
               dst_path: &Path,
               chown: &Option<String>,
               chmod: &Option<String>,
               host: &Host) -> FileCopyOperation {
        FileCopyOperation {
            operation,
            engine,
            bin_id,
            curr_dir: curr_dir.to_owned(),
            src_path: src_path.to_owned(),
            dst_path: dst_path.to_owned(),
            chown: chown.as_ref().map(|s|s.to_string()),
            chmod: chmod.as_ref().map(|s|s.to_string()),
            host: host.clone(),
            status: Arc::new(Mutex::new(None)),
            running: false
        }
    }

    fn prepare_params(&self) -> Result<RsyncParams, CommandError>{
        let ssh_session = self.engine.write().ssh_session_cache_mut().get(self.host.ssh_dest())?;
        let mut params = RsyncParams::new(&self.curr_dir, &self.src_path, &self.dst_path);
        params
            //.dst_hostname(self.host.ssh_dest().hostname())
            .remote_shell(ssh_session.read().remote_shell_call());
        if let Some(chown) = &self.chown {
            params.chown(chown.to_owned());
        }
        if let Some(chmod) = &self.chmod {
            params.chmod(chmod.to_owned());
        }
        Ok(params)
    }

    fn spawn_std_watchers(&self) -> Result<(PipeWriter, PipeWriter), CommandError>{
        use std::io::BufRead;
        let (stdout, stdout_writer) = pipe()?;
        let (stderr, stderr_writer) = pipe()?;

        let operation = self.operation.clone();

        let run_stdout = move || {
            let mut buf = BufReader::new(stdout);

//            for line in buf.lines() {
//                match line {
//                    Ok(line) => {
//                        println!("out: {}", line);
//                        operation.write().update_progress_value(1.0);
//                    },
//                    Err(err) => return Err(err),
//                }
//            }
            if let Err(err) = parse_progress(&mut buf){
                println!("Error parsing rsync progress: {:?}", err)
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

        let hout: JoinHandle<std::io::Result<()>> = std::thread::spawn(run_stdout);
        let herr: JoinHandle<std::io::Result<()>> = std::thread::spawn(run_stderr);
        Ok((stdout_writer, stderr_writer))
    }

    fn start_copying(&mut self) -> Result<(), RuntimeError>{
        let params = self.prepare_params()?;
        let config = self.engine.read().config().exec().file().rsync().clone();
        let (stdout, stderr) = self.spawn_std_watchers()?;

        let status = self.status.clone();
        let operation = self.operation.clone();

        std::thread::spawn(move || {
            let execute_cmd = move || -> Result<ExitStatus, RuntimeError> {
                let mut command = params.to_cmd(&config);
                command.arg("--progress")
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

                let mut child = command.spawn()?;
                    Ok(child.wait()?)
            };

            match execute_cmd() {
                Ok(stat) => {
                    *status.lock().unwrap() = Some(Ok(stat))
                }
                Err(err) => {
                    *status.lock().unwrap() = Some(Err(err))
                }
            }
            operation.write().notify()
        });
        Ok(())
    }
    pub fn status(&self) -> MutexGuard<Option<Result<ExitStatus, RuntimeError>>>{
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
            return Ok(Async::NotReady)
        }

        match *self.status() {
            Some(Ok(ref status)) =>{
                if status.success() {
                    Ok(Async::Ready(Outcome::Empty))
                } else {
                    Err(RuntimeError::Custom)
                }
            }
            Some(Err(ref err)) => {
                Err(RuntimeError::Custom)
            }
            None => Ok(Async::NotReady)
        }
    }
}

impl OperationImpl for FileCopyOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
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

#[allow(dead_code)]
#[cfg(test)]
mod tests {
    use super::*;

    //    #[test]
    /*fn send_test() {
        let auth = Auth::new("wsikora", "/home/wsikora/Desktop/id_rsa");

        let host = Host {
            hostname: "localhost".to_string(),
            fqdn: "localhost.localdomain".to_string(),
            addr: "127.0.0.1".parse().unwrap(),
            ssh_port: 22,
        };

        let rsync_send = RsyncSend::new();

        rsync_send.on_complete(|res| {
            eprintln!("On complete with result = {:#?}", res);
        });

        rsync_send.on_file_begin(|file_name, file_size| {
            eprintln!("New file begin: {}, size: {}", file_name, file_size);
        });

        rsync_send.on_file_complete(|file_name| {
            eprintln!("File complete = {:?}", file_name);
        });

        rsync_send.on_progress(|progress, file_size, file_name| {
            eprintln!(" Progress: file {} : {} / {}", file_name, progress, file_size);
        });

        rsync_send.on_parse_error(|| {
            eprintln!("Rsync output parse multi.rs");
        });


        let mut cfg = Config::new(
            &"./test-data/src/".into(),
            &"/home/wsikora/Desktop/kodegenie/op-exec/test-data/dst".into());
        cfg
            .chmod("a+w")
            .dst_user("wsikora")
            .dst_host("localhost");
//            .add_src_path(&PathBuf::from("./test-data/src/idea.tar.gz"));

        eprintln!("&cfg = {:#?}", &cfg);

        let sessions = SessionCache::new(1);

        let res = rsync_send.send(&host, &auth, cfg, sessions).unwrap().join().unwrap();
        res.unwrap();
    }*/

    #[test]
    fn rsync_copy_() {
        let config = RsyncConfig::default();
        let p = RsyncParams::new("./","/home/outsider/Down1/", "/home/outsider/Down2/");
        //p.remote_shell("/bin/ssh ssh://localhost -i ~/.ssh/id_rsa -S /home/outsider/.operon/run/ssh/outsider-127.0.0.1-22.sock -T -o StrictHostKeyChecking=yes");

        let status = rsync_copy(&config, &p).unwrap();
        println!("{:?}", status);
    }
}

