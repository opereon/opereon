use super::*;

mod config;
mod command;
mod file;
mod template;

pub use self::config::*;
pub use self::command::*;
pub use self::file::*;
pub use self::template::*;
pub use self::file::FileCopyOperation;

use std::sync::{Arc, Mutex, MutexGuard};
use std::io::{Read, Write, Seek, SeekFrom};

pub trait ReadWriteSeek: Read + Write + Seek + 'static { }

impl<T: Read + Write + Seek + 'static> ReadWriteSeek for T { }


#[derive(Clone)]
pub struct OutputLog(Arc<Mutex<Box<dyn ReadWriteSeek>>>);

impl OutputLog {
    pub fn new<B: ReadWriteSeek>(inner: B) -> OutputLog {
        OutputLog(Arc::new(Mutex::new(Box::new(inner))))
    }

    fn lock(&self) -> MutexGuard<Box<dyn ReadWriteSeek>> {
        self.0.lock().unwrap()
    }

    pub fn log_cmd(&self, cmd: &str) -> std::io::Result<()> {
        let mut out = self.lock();
        for line in cmd.lines() {
            out.write_all(b"$ ")?;
            out.write_all(line.as_bytes())?;
            out.write_all(b"\n")?;
        }
        Ok(())
    }

    fn log_stream<S: Read>(&self, stream: S, prefix: &[u8]) -> std::io::Result<()> {
        use std::io::BufRead;

        let mut out = self.lock();
        let mut line = String::new();
        let mut buf = BufReader::new(stream);
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
    }

    pub fn log_stdin<S: Read>(&self, stdin: S) -> std::io::Result<()> {
        self.log_stream(stdin, b"< ")
    }

    pub fn log_stdout<S: Read>(&self, stdout: S) -> std::io::Result<()> {
        self.log_stream(stdout, b"1 ")
    }

    pub fn log_stderr<S: Read>(&self, stderr: S) -> std::io::Result<()> {
        self.log_stream(stderr, b"2 ")
    }

    pub fn log_status(&self, code: Option<i32>) -> std::io::Result<()> {
        let mut out = self.lock();
        out.write_all(b"= ")?;
        match code {
            Some(code) => out.write_all(code.to_string().as_bytes())?,
            None => out.write_all(b"?")?,
        };
        out.write_all(b"\n")?;
        Ok(())
    }

    pub fn rewind(&self) -> std::io::Result<()> {
        let mut out = self.lock();
        out.seek(SeekFrom::Start(0))?;
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
