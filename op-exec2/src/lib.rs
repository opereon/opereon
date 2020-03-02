#![feature(box_syntax, specialization, integer_atomics)]

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

#[macro_use]
extern crate op_log;

#[cfg(test)]
#[macro_use]
extern crate pretty_assertions;

use std::path::{Path, PathBuf};

use chrono::prelude::*;
use kg_diag::io::fs;
use kg_diag::*;

use kg_utils::collections::{LinkedHashMap, LruCache};

mod command;
mod outlog;

pub use self::outlog::{EntryKind, OutputLog};
