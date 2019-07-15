extern crate slog;
extern crate colored;

use std::fs::OpenOptions;
use slog::{Drain, Record, OwnedKVList, Never};
use slog::*;
use slog_kvfilter::KVFilter;
use std::collections::{HashSet, HashMap};
use std::path::Path;
use crate::colored::Colorize;

const VERBOSITY_KEY: &str = "verbosity";

pub fn build_file_drain<P: AsRef<Path>>(log_path: P, level: Level) -> impl Drain<Ok=(), Err=Never> {
    if let Some(log_dir) = log_path.as_ref().parent() {
        std::fs::create_dir_all(log_dir).expect("Cannot create log dir");
    }

    let mut open_opts = OpenOptions::new();

    open_opts.create(true).append(true);

    let log_file = open_opts.open(log_path).expect("Cannot open log file");

    let decorator = slog_term::PlainSyncDecorator::new(log_file.try_clone().unwrap());
    let drain = slog_term::FullFormat::new(decorator).build();
    let drain = slog::LevelFilter::new(drain, level);
    drain.fuse()
}

/// Creates drain printing to stdout.
/// Only messages with `verbosity` key will be printed.
///
pub fn build_cli_drain(verbosity: u8) -> impl Drain<Ok=(), Err=Never> {
    let mut verbosity_vals: HashSet<String> = HashSet::new();

    for v in 0..verbosity + 1 {
        verbosity_vals.insert(format!("{}", v));
    }

    let mut filters = HashMap::new();
    filters.insert(VERBOSITY_KEY.into(), verbosity_vals);

    let drain = CliDrain;
    let drain = KVFilter::new(drain, Level::Error);
    let drain = drain.only_pass_any_on_all_keys(Some(filters));
//    let drain = slog::Filter(drain, |r| r.level() == Level::Info);
    drain.fuse()
}


pub struct CliDrain;

/// Drain for printing log messages directly to the user.
/// Currently just prints colored message to stdout.
impl Drain for CliDrain {
    type Ok = ();
    type Err = ();

    fn log(
        &self,
        record: &Record,
        _values: &OwnedKVList,
    ) -> std::result::Result<Self::Ok, Self::Err> {

        let prefix = match record.level() {
            Level::Critical => {
                "Critical:".red()
            },
            Level::Error => {
                "Error:".bright_red()
            },
            Level::Warning => {
                "Warn:".yellow()
            },
            Level::Info => {
                "Info:".blue()
            },
            Level::Debug => {
                "Debug:".cyan()
            }
            Level::Trace => {
                "Trace:".white()
            }
        };

        println!("{} {}", prefix, record.msg());

//        record
//            .kv()
//            .serialize(record, &mut CliSerializer)
//            .unwrap();
//        values.serialize(record, &mut CliSerializer).unwrap();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slog_kvfilter::KVFilter;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn cli_drain_test() {
        let drain = build_cli_drain(2);

        let log = slog::Logger::root(
            drain.fuse(),
            o!("module" =>
           FnValue(move |info| {
                info.module()
           })
          ),
        );


        crit!(log, "CRIT! verbosity is {verbosity}", verbosity = 0);
        error!(log, "ERR! verbosity is "; "verbosity" => 0);
        warn!(log, "WARN! verbosity is {verbosity}", verbosity = 0);
        info!(log, "INFO! verbosity is {verbosity}", verbosity = 1);
        debug!(log, "DEBUG! verbosity is {verbosity}", verbosity = 0);
        trace!(log, "TRACE! verbosity is {verbosity}", verbosity = 2);

        info!(log, "INFO! info message! - KV syntax"; "verbosity" => 1);


        info!(log, "verbosity not specified!");
        warn!(log, "verbosity is {foo} {bar}", bar=3, foo = 2; "a" => "b");
        debug!(log, "formatted {num_entries} entries of {}", "something", num_entries = 2; "log-key" => true);
        trace!(log, "{first} {third} {second}", first = 1, second = 2, third=3; "forth" => 4, "fifth" => 5);
    }
}
