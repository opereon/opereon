#![feature(
    box_syntax,
    specialization,
    integer_atomics,
)]

#[macro_use]
extern crate kg_diag_derive;
#[macro_use]
extern crate kg_display_derive;
extern crate kg_utils;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate slog;

use std::io::BufReader;
use std::path::{Path, PathBuf};

use chrono::prelude::*;
use kg_diag::*;
use kg_io::*;
use kg_io::fs;
use kg_symbol::Symbol;
use kg_tree::*;
use kg_tree::diff::*;
use kg_tree::opath::*;
use kg_tree::serial::from_tree;
use kg_utils::collections::{LinkedHashMap, LruCache};
use os_pipe::pipe;
use uuid::Uuid;

use op_model::*;

pub use self::config::*;
pub use self::core::*;
pub use self::exec::*;
pub use self::proto::*;

mod config;
mod proto;
mod core;
mod exec;

