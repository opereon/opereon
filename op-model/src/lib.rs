#![feature(box_syntax, specialization, raw)]

#[cfg(test)]
#[macro_use]
extern crate indoc;
#[macro_use]
extern crate serde_derive;

extern crate parking_lot;

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
