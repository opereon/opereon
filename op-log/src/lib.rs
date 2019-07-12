#[macro_use]
extern crate slog;

use std::fs::OpenOptions;
use slog::{Drain, Record, OwnedKVList, Serializer, Never};
use slog::FnValue;
use slog::*;
use slog_kvfilter::KVFilter;
use std::collections::{HashSet, HashMap};
use core::fmt;
use std::path::Path;

const VERBOSITY_KEY: &str = "verbosity";

/// Log info level record with verbosity 0.
///
#[macro_export(local_inner_macros)]
macro_rules! info0(
    ($l:expr, $($args:tt)*) => {
        slog::info!($l, $( $args)*; "verbosity"=> 0 )
    };
    ($l:expr; $($kvs:tt)*) => {
        slog::info!($l; $( $kvs)*, "verbosity"=> 0 )
    };
    ($l:expr, $($args:tt)* ; $($kvs:tt)+) => {
        slog::info!($l, $( $args)*; $($kvs)*, "verbosity"=> 0 )
    };
);


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
    let drain = slog::Filter(drain, |r| r.level() == Level::Info);
    drain.fuse()
}

//pub struct CliSerializer;
//
//impl Serializer for CliSerializer {
//    fn emit_arguments(&mut self, key: Key, val: &fmt::Arguments) -> Result {
//        print!("{}={}", key, val);
//        Ok(())
//    }
//}

pub struct CliDrain;

/// Drain for printing log messages directly to the user.
/// Currently just prints message to stdout.
impl Drain for CliDrain {
    type Ok = ();
    type Err = ();

    fn log(
        &self,
        record: &Record,
        values: &OwnedKVList,
    ) -> std::result::Result<Self::Ok, Self::Err> {
        println!("{}", record.msg());

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
        let drain = build_cli_drain(0);

        let log = slog::Logger::root(
            drain.fuse(),
            o!("module" =>
           FnValue(move |info| {
                info.module()
           })
          ),
        );

        info!(log, "verbosity is {verbosity}", verbosity = 0);
        info!(log, "verbosity is {verbosity}", verbosity = 1);
        info!(log, "verbosity is {verbosity}", verbosity = 2);
        info!(log, "verbosity not specified!");
        warn!(log, "verbosity is {foo} {bar}", bar=3, foo = 2; "a" => "b");
        debug!(log, "formatted {num_entries} entries of {}", "something", num_entries = 2; "log-key" => true);
        trace!(log, "{first} {third} {second}", first = 1, second = 2, third=3; "forth" => 4, "fifth" => 5);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
