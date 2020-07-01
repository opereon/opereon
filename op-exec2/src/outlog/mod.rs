use super::*;

use parking_lot::Mutex;

use std::io::{BufRead, BufReader, Read};
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(u8)]
pub enum EntryKind {
    Out = 0x01,
    Err = 0x02,
    In = 0x04,
    Status = 0x08,
    Command = 0x10,
}

#[derive(Clone, Default)]
pub struct OutputLog(Option<Arc<Mutex<Output>>>);

impl OutputLog {
    pub fn new() -> OutputLog {
        OutputLog(Some(Arc::new(Mutex::new(Output::new()))))
    }

    pub fn null() -> OutputLog {
        OutputLog(None)
    }

    pub fn log_entry(&self, kind: EntryKind, timestamp: Instant, data: &[u8]) -> IoResult<()> {
        if let Some(ref o) = self.0 {
            let mut o = o.lock();
            o.log_entry(kind, timestamp, data)
        } else {
            Ok(())
        }
    }

    pub fn log_entry_disp<T: std::fmt::Display>(
        &self,
        kind: EntryKind,
        timestamp: Instant,
        data: T,
    ) -> IoResult<()> {
        if let Some(ref o) = self.0 {
            let mut o = o.lock();
            o.log_entry_disp(kind, timestamp, data)
        } else {
            Ok(())
        }
    }

    pub fn log_entry_now(&self, kind: EntryKind, data: &[u8]) -> IoResult<()> {
        self.log_entry(kind, Instant::now(), data)
    }

    pub fn log_in(&self, data: &[u8]) -> IoResult<()> {
        self.log_entry_now(EntryKind::In, data)
    }

    pub fn log_out(&self, data: &[u8]) -> IoResult<()> {
        self.log_entry_now(EntryKind::Out, data)
    }

    pub fn log_err(&self, data: &[u8]) -> IoResult<()> {
        self.log_entry_now(EntryKind::Err, data)
    }

    pub fn log_command(&self, data: &[u8]) -> IoResult<()> {
        self.log_entry_now(EntryKind::Command, data)
    }

    pub fn log_status(&self, status: Option<i32>) -> IoResult<()> {
        match status {
            Some(value) => self.log_entry_disp(EntryKind::Status, Instant::now(), value),
            None => self.log_entry_disp(EntryKind::Status, Instant::now(), '?'),
        }
    }

    pub fn consume_stderr<R: Read>(&self, stderr: R) -> IoResult<()> {
        self.consume_input(stderr, EntryKind::Err)
    }

    pub fn consume_stdout<R: Read>(&self, stderr: R) -> IoResult<()> {
        self.consume_input(stderr, EntryKind::Out)
    }

    fn consume_input<R: Read>(&self, reader: R, kind: EntryKind) -> IoResult<()> {
        let r = BufReader::new(reader);
        let lines = r.lines();

        for res in lines {
            match res {
                Ok(line) => {
                    self.log_entry_now(kind, line.as_bytes())?;
                }
                Err(err) => {
                    // TODO ws what to do with error?
                    eprintln!("Error reading output = {:?}", err);
                    // keep draining reader to prevent main process hang/failure
                }
            }
        }
        Ok(())
    }
}

impl std::fmt::Display for OutputLog {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(ref o) = self.0 {
            let o = o.lock();
            std::fmt::Display::fmt(&o, f)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Position {
    offset: usize,
    length: usize,
}

impl std::fmt::Display for EntryKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            EntryKind::In => write!(f, "<"),
            EntryKind::Out => write!(f, "1"),
            EntryKind::Err => write!(f, "2"),
            EntryKind::Status => write!(f, "="),
            EntryKind::Command => write!(f, "$"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    pos: Position,
    kind: EntryKind,
    timestamp: Instant,
}

struct Output {
    buf: Vec<u8>,
    entries: Vec<Entry>,
}

impl Output {
    fn new() -> Output {
        Output {
            buf: Vec::new(),
            entries: Vec::new(),
        }
    }

    fn log_entry(&mut self, kind: EntryKind, timestamp: Instant, data: &[u8]) -> IoResult<()> {
        self.entries.push(Entry {
            pos: Position {
                offset: self.buf.len(),
                length: data.len(),
            },
            kind,
            timestamp,
        });
        self.buf.extend_from_slice(data);
        Ok(())
    }

    fn log_entry_disp<T: std::fmt::Display>(
        &mut self,
        kind: EntryKind,
        timestamp: Instant,
        data: T,
    ) -> IoResult<()> {
        use std::io::Write;
        let mut entry = Entry {
            pos: Position {
                offset: self.buf.len(),
                length: 0,
            },
            kind,
            timestamp,
        };
        write!(self.buf, "{}", data).unwrap();
        entry.pos.length = self.buf.len() - entry.pos.offset;
        self.entries.push(entry);
        Ok(())
    }
}

impl std::fmt::Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for e in self.entries.iter() {
            let s = &self.buf[e.pos.offset..(e.pos.offset + e.pos.length)];
            let s = String::from_utf8_lossy(s);
            writeln!(f, "{} {}", e.kind, s)?;
        }
        Ok(())
    }
}

/*
pub struct OutputLogReader {
    log: OutputLog,
    kind_mask: u8,
    entry_index: usize,
    entry_offset: usize,
}

impl OutputLogReader {
    pub fn new(log: &OutputLog, kind_mask: u8) -> OutputLogReader {
        OutputLogReader {
            log: log.clone(),
            kind_mask,
            entry_index: 0,
            entry_offset: 0,
        }
    }
}

impl std::io::Read for OutputLogReader {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let log = self.log.lock()
    }
}
*/

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_outlog() {
        let log = OutputLog::new();
        log.log_command(b"echo 'test'").unwrap();
        log.log_out(b"test").unwrap();
        log.log_err(b"unknown error").unwrap();
        log.log_status(Some(0)).unwrap();
        log.log_status(None).unwrap();
        println!("{}", log);
    }
}
