#![feature(box_syntax, specialization, integer_atomics)]

#[macro_use]
extern crate pin_project;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate slog;
#[macro_use]
extern crate futures;
extern crate async_trait;

#[macro_use]
extern crate kg_diag_derive;
#[macro_use]
extern crate kg_display_derive;
extern crate kg_utils;
extern crate op_engine;

#[macro_use]
extern crate op_log;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

use std::path::Path;

use kg_diag::io::fs;
use kg_diag::*;

use kg_utils::collections::{LinkedHashMap, LruCache};

pub mod command;
pub mod outlog;
pub mod rsync;
mod utils;

pub use self::outlog::{EntryKind, OutputLog};
