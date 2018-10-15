#![feature(box_syntax, specialization, raw)]

#[cfg(test)]
#[macro_use]
extern crate indoc;

extern crate walkdir;
extern crate globset;
extern crate uuid;
extern crate heapsize;
extern crate chrono;
extern crate users;
extern crate crypto;

#[macro_use]
extern crate serde_derive;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate toml;

extern crate kg_io;
extern crate kg_tree;
extern crate kg_symbol;
#[macro_use]
extern crate kg_utils;


use std::path::{Path, PathBuf};

use chrono::prelude::*;

use kg_utils::collections::LinkedHashMap;
use kg_io::*;
use kg_tree::*;
use kg_tree::opath::*;
use kg_tree::diff::*;
use kg_symbol::Symbol;

mod config;
mod manifest;
mod metadata;
mod model;
mod defs;
mod update;

use self::config::*;
use self::defs::Scoped;
pub use self::manifest::*;
pub use self::metadata::*;
pub use self::model::*;
pub use self::defs::*;
pub use self::update::*;
