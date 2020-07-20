#![deny(unused_extern_crates)]
#![feature(box_syntax, min_specialization, integer_atomics)]

#[macro_use]
extern crate pin_project;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate futures;

#[macro_use]
extern crate kg_diag_derive;
#[macro_use]
extern crate kg_display_derive;

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
pub mod utils;

pub use self::outlog::{EntryKind, OutputLog};
