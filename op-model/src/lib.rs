#![feature(box_syntax, specialization, raw)]

#[cfg(test)]
#[macro_use]
extern crate indoc;
extern crate parking_lot;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate kg_diag_derive;
#[macro_use]
extern crate kg_display_derive;

use std::path::{Path, PathBuf};

use chrono::prelude::*;
use kg_io::*;
use kg_symbol::Symbol;
use kg_tree::diff::*;
use kg_tree::opath::*;
use kg_tree::*;
use kg_utils::collections::LinkedHashMap;

use self::config::*;
use self::defs::Scoped;
pub use self::defs::*;
pub use self::git::*;
pub use self::manifest::*;
pub use self::metadata::*;
pub use self::model::*;
pub use self::update::*;

mod config;
mod defs;
mod git;
mod manifest;
mod metadata;
mod model;
mod update;
