use lazy_static::*;
use std::sync::{Arc, Mutex};

pub trait Logger: Send + 'static {
    fn log(&mut self, record: &Record);
}

pub struct DiscardLogger;

impl Logger for DiscardLogger {
    fn log(&mut self, _record: &Record) {}
}

lazy_static! {
    static ref OP_LOGGER: Arc<Mutex<Box<dyn Logger>>> = {
        Arc::new(Mutex::new(Box::new(DiscardLogger)))
    };
}

pub fn set_logger<L: Logger>(logger: L) {
    *OP_LOGGER.lock().unwrap() = Box::new(logger);
}

#[doc(hidden)]
pub fn __private_api_log(
    msg: String,
    level: Level,
    verbosity: usize,
) {
    let r = Record {
        msg,
        level,
        verbosity
    };
    OP_LOGGER.lock().unwrap().log(&r);
}

#[derive(Copy, Clone, Debug, Hash, Eq)]
pub enum Level {
    Info = 0,
    Warn = 1,
    Error = 2,
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        match self {
            Level::Info => {f.write_str("Info")},
            Level::Warn => {f.write_str("Warn")},
            Level::Error => {f.write_str("Error")},
        }
    }
}

impl PartialEq for Level {
    #[inline]
    fn eq(&self, other: &Level) -> bool {
        *self as usize == *other as usize
    }
}

#[derive(Clone, Debug, Hash)]
pub struct Record {
    msg: String,
    level: Level,
    verbosity: usize,
}

impl Record {
    pub fn msg(&self) -> &str {
        &self.msg
    }
    pub fn level(&self) -> Level {
        self.level
    }
    pub fn verbosity(&self) -> usize {
        self.verbosity
    }
}

#[macro_export(local_inner_macros)]
macro_rules! op_log {
    ($verbosity: expr, $lvl:expr, $($arg:tt)+) => ({
        let lvl = $lvl;
        let msg = std::format!($($arg)+);
        let verbosity = $verbosity;
        $crate::__private_api_log(
            msg,
            lvl,
            verbosity
        );
    });
}

#[macro_export(local_inner_macros)]
macro_rules! op_info {
    ($verbosity: expr, $($arg:tt)+) => {
        $crate::op_log!($verbosity, $crate::Level::Info, $($arg)+)
    };
}

#[macro_export(local_inner_macros)]
macro_rules! op_warn {
    ($verbosity: expr, $($arg:tt)+) => {
        $crate::op_log!($verbosity, $crate::Level::Warn, $($arg)+)
    };
}

#[macro_export(local_inner_macros)]
macro_rules! op_error {
    ($verbosity: expr, $($arg:tt)+) => {
        $crate::op_log!($verbosity, $crate::Level::Error, $($arg)+)
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestLogger {
        records: Arc<Mutex<Vec<Record>>>
    }

    impl TestLogger {
        pub fn new(records: Arc<Mutex<Vec<Record>>>) -> TestLogger {
            TestLogger {
                records
            }
        }
    }

    impl Logger for TestLogger{
        fn log(&mut self, record: &Record) {
            self.records.lock().unwrap().push(record.clone())
        }
    }

    #[test]
    fn foo() {
        let records = Arc::new(Mutex::new(vec![]));
        set_logger(TestLogger::new(records.clone()));

        op_info!(0, "formatting {}", "some value");
        op_warn!(1, "formatting {named_value}", named_value="val");
        op_error!(2, "formatting {} {}", "some value", "another value");

        let records: Vec<Record> = records.lock().unwrap().drain(..).collect();

        assert_eq!(0, records[0].verbosity());
        assert_eq!(1, records[1].verbosity());
        assert_eq!(2, records[2].verbosity());

        assert_eq!(Level::Info, records[0].level());
        assert_eq!(Level::Warn, records[1].level());
        assert_eq!(Level::Error, records[2].level());

        assert_eq!("formatting some value", records[0].msg());
        assert_eq!("formatting val", records[1].msg());
        assert_eq!("formatting some value another value", records[2].msg());
    }
}