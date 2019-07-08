#![feature(box_syntax, specialization, raw)]

#[cfg(test)]
#[macro_use]
extern crate indoc;
extern crate parking_lot;
#[macro_use]
extern crate serde_derive;

use std::path::{Path, PathBuf};

use chrono::prelude::*;
use kg_io::*;
use kg_symbol::Symbol;
use kg_tree::*;
use kg_tree::diff::*;
use kg_tree::opath::*;
use kg_utils::collections::LinkedHashMap;

use self::config::*;
pub use self::defs::*;
use self::defs::Scoped;
pub use self::manifest::*;
pub use self::metadata::*;
pub use self::model::*;
pub use self::update::*;

mod config;
mod manifest;
mod metadata;
mod model;
mod defs;
mod update;

