#![feature(
    mpsc_select,
    box_syntax,
    specialization,
    integer_atomics,
)]

#[macro_use]
extern crate lazy_static;
extern crate tokio;
extern crate tokio_process;
extern crate os_pipe;
extern crate time;
extern crate chrono;
extern crate libc;
extern crate regex;
extern crate uuid;
extern crate walkdir;
extern crate hostname;
extern crate rand;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serde_json;
extern crate serde_yaml;
extern crate toml;
extern crate rmp_serde;

extern crate crypto;
#[macro_use]
extern crate slog;

#[macro_use]
extern crate kg_diag;
#[macro_use]
extern crate kg_diag_derive;
#[macro_use]
extern crate kg_display_derive;
#[macro_use]
extern crate kg_utils;
extern crate kg_io;
extern crate kg_tree;
extern crate kg_template;
extern crate kg_symbol;
extern crate op_model;
extern crate op_net;


use std::io::{BufReader};
use std::path::{Path, PathBuf};

use os_pipe::pipe;
use chrono::prelude::*;
use uuid::Uuid;

use kg_utils::collections::{LinkedHashMap, LruCache};
use kg_io::*;
use kg_diag::*;
use kg_symbol::Symbol;
use kg_tree::*;
use kg_tree::opath::*;
use kg_tree::serial::{from_tree};
use kg_tree::diff::*;
use op_model::*;

mod config;
mod proto;
mod core;
mod exec;

pub use self::config::*;
pub use self::proto::*;
pub use self::exec::*;
pub use self::core::*;
