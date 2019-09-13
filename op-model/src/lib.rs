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
#[macro_use]
extern crate slog;

use std::path::{Path, PathBuf};

use kg_diag::io::fs;
use kg_diag::*;
use kg_symbol::Symbol;
use kg_tree::diff::*;
use kg_tree::opath::*;
use kg_tree::*;
use kg_utils::collections::LinkedHashMap;
use op_rev::*;
use slog::{o, warn, Logger};

pub static DEFAULT_CONFIG_FILENAME: &'static str = ".operc";
pub static DEFAULT_MANIFEST_FILENAME: &'static str = "op.toml";
pub static DEFAULT_WORK_DIR_PATH: &'static str = ".op/";

// language=toml
pub static INITIAL_MANIFEST: &'static str = r#"
[info]
authors = [""]
description = ""
"#;

// language=toml
pub static INITIAL_CONFIG: &'static str = r#"
[[exclude]]
path = "*.sh"
"#;


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


fn init_manifest(model_dir: &Path, logger: &Logger) -> ModelResult<()> {
    let manifest_path = model_dir.join(DEFAULT_MANIFEST_FILENAME);
    if manifest_path.exists() {
        info!(logger, "Manifest file '{manifest}' already exists, skipping...", manifest=manifest_path.display(); "verbosity"=>1);
        return Ok(())
    }

    fs::write(manifest_path, INITIAL_MANIFEST)?;
    Ok(())
}

fn init_config(model_dir: &Path, logger: &Logger) -> ModelResult<()> {
    let config_path = model_dir.join(DEFAULT_CONFIG_FILENAME);

    if config_path.exists() {
        info!(logger, "Config file '{config}' already exists, skipping...", config=config_path.display(); "verbosity"=>1);
        return Ok(())
    }

    fs::write(config_path, INITIAL_CONFIG)?;
    Ok(())
}
