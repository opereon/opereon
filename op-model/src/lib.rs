//#![deny(warnings)]
#![feature(box_syntax, specialization, raw)]

#[cfg(test)]
#[macro_use]
extern crate indoc;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate kg_diag_derive;
#[macro_use]
extern crate kg_display_derive;

use std::path::{Path, PathBuf};

use kg_diag::io::fs;
use kg_diag::*;
use kg_symbol::Symbol;
use kg_tree::diff::*;
use kg_tree::opath::*;
use kg_tree::*;
use kg_utils::collections::LinkedHashMap;
use op_rev::*;

pub static DEFAULT_CONFIG_FILENAME: &'static str = ".operc";
pub static DEFAULT_MANIFEST_FILENAME: &'static str = "op.toml";
pub static DEFAULT_WORK_DIR_PATH: &'static str = ".op/";

pub use self::config::*;
pub use self::defs::*;
pub use self::load_file::*;
pub use self::manifest::*;
pub use self::model::*;
pub use self::update::*;

mod config;
mod defs;
mod load_file;
mod manifest;
mod model;
mod update;
