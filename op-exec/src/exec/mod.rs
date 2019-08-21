use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex, MutexGuard};

use super::*;

pub use self::command::*;
pub use self::config::*;
pub use self::file::FileCopyOperation;
pub use self::file::*;
pub use self::template::*;
use kg_diag::io::ResultExt;

mod command;
mod config;
mod file;
mod template;

pub type OutputLogErr = BasicDiag;
pub type OutputLogResult<T> = Result<T, OutputLogErr>;

#[derive(Debug, Display, Detail)]
pub enum OutputLogErrDetail {
    #[display(fmt = "cannot log command: {cmd}")]
    LogCmd { cmd: String },

    #[display(fmt = "cannot log stream")]
    LogStream,

    #[display(fmt = "cannot log status")]
    LogStatus,

    #[display(fmt = "cannot rewind log")]
    LogRewind,
}

pub trait ReadWriteSeek: Read + Write + Seek + 'static {}

impl<T: Read + Write + Seek + 'static> ReadWriteSeek for T {}

#[derive(Clone)]
pub struct OutputLog(Arc<Mutex<Box<dyn ReadWriteSeek>>>);

pub fn execute_io<F: FnMut() -> std::io::Result<()>>(func: F) -> Result<(), BasicDiag> {
    func().map_err_to_diag()
}

impl OutputLog {
    pub fn new<B: ReadWriteSeek>(inner: B) -> OutputLog {
        OutputLog(Arc::new(Mutex::new(Box::new(inner))))
    }

    fn lock(&self) -> MutexGuard<Box<dyn ReadWriteSeek>> {
        self.0.lock().unwrap()
    }

    pub fn log_cmd(&self, cmd: &str) -> OutputLogResult<()> {
        let mut out = self.lock();
        let res = execute_io(|| {
            for line in cmd.lines() {
                out.write_all(b"$ ")?;
                out.write_all(line.as_bytes())?;
                out.write_all(b"\n")?;
            }
            Ok(())
        });
        res.map_err_as_cause(|| OutputLogErrDetail::LogCmd {
            cmd: cmd.to_string(),
        })
    }

    fn log_stream<S: Read>(&self, stream: S, prefix: &[u8]) -> OutputLogResult<()> {
        use std::io::BufRead;

        let mut out = self.lock();
        let mut line = String::new();
        let mut buf = BufReader::new(stream);
        execute_io(|| {
            loop {
                line.clear();
                let len = buf.read_line(&mut line)?;
                if len == 0 {
                    break;
                }
                out.write_all(prefix)?;
                out.write_all(line.as_bytes())?;
                if !line.ends_with('\n') {
                    out.write_all(b"\n")?;
                }
            }
            Ok(())
        })
        .map_err_as_cause(|| OutputLogErrDetail::LogStream)
    }

    pub fn log_stdin<S: Read>(&self, stdin: S) -> OutputLogResult<()> {
        self.log_stream(stdin, b"< ")
    }

    pub fn log_stdout<S: Read>(&self, stdout: S) -> OutputLogResult<()> {
        self.log_stream(stdout, b"1 ")
    }

    pub fn log_stderr<S: Read>(&self, stderr: S) -> OutputLogResult<()> {
        self.log_stream(stderr, b"2 ")
    }

    pub fn log_status(&self, code: Option<i32>) -> OutputLogResult<()> {
        let mut out = self.lock();
        execute_io(|| {
            out.write_all(b"= ")?;
            match code {
                Some(code) => out.write_all(code.to_string().as_bytes())?,
                None => out.write_all(b"?")?,
            };
            out.write_all(b"\n")?;
            Ok(())
        })
        .map_err_as_cause(|| OutputLogErrDetail::LogStatus)
    }

    pub fn rewind(&self) -> OutputLogResult<()> {
        let mut out = self.lock();
        out.seek(SeekFrom::Start(0))
            .map_err_to_diag()
            .map_err_as_cause(|| OutputLogErrDetail::LogRewind)?;
        Ok(())
    }
}

impl std::fmt::Display for OutputLog {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use std::io::BufRead;

        let mut out = self.lock();
        out.seek(SeekFrom::Start(0)).unwrap(); //FIXME (jc)
        let mut r = BufReader::new(&mut *out);
        let mut line = String::new();
        loop {
            line.clear();
            let len = r.read_line(&mut line).unwrap(); //FIXME (jc)
            if len == 0 {
                break;
            }
            write!(f, "{}", line)?;
        }
        Ok(())
    }
}

unsafe impl Send for OutputLog {}

unsafe impl Sync for OutputLog {}
