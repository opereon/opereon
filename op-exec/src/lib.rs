#![feature(
    box_syntax,
    specialization,
    integer_atomics,
)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate slog;
#[macro_use]
extern crate kg_diag_derive;
#[macro_use]
extern crate kg_display_derive;
#[macro_use]
extern crate kg_utils;

use std::io::{BufReader};
use std::path::{Path, PathBuf};

use os_pipe::pipe;
use chrono::prelude::*;
use uuid::Uuid;

use kg_utils::collections::{LinkedHashMap, LruCache};
use kg_io::*;
use kg_io::fs;
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
