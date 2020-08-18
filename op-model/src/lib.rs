//#![deny(warnings)]
#![feature(box_syntax, min_specialization, raw)]

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
extern crate tracing;

use std::path::{Path, PathBuf};

use kg_diag::io::fs;
use kg_diag::*;
use kg_symbol::Symbol;
use kg_tree::diff::*;
use kg_tree::opath::*;
use kg_tree::*;
use kg_utils::collections::LinkedHashMap;
use op_rev::*;

pub static DEFAULT_CONFIG_FILENAME: &str = ".operc";
pub static DEFAULT_MANIFEST_FILENAME: &str = "op.toml";
pub static DEFAULT_WORK_DIR_PATH: &str = ".op/";

// language=toml
pub static INITIAL_MANIFEST: &str = r#"
[info]
authors = [""]
description = ""
"#;

// language=toml
pub static INITIAL_CONFIG: &str = r#"
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


fn init_manifest(model_dir: &Path) -> ModelResult<()> {
    let manifest_path = model_dir.join(DEFAULT_MANIFEST_FILENAME);
    if manifest_path.exists() {
        info!(verb=1, ?manifest_path, "Manifest file already exists, skipping...");
        return Ok(());
    }

    fs::write(manifest_path, INITIAL_MANIFEST)?;
    Ok(())
}

fn init_config(model_dir: &Path) -> ModelResult<()> {
    let config_path = model_dir.join(DEFAULT_CONFIG_FILENAME);

    if config_path.exists() {
        info!(verb=1, ?config_path, "Config file already exists, skipping...");
        return Ok(());
    }

    fs::write(config_path, INITIAL_CONFIG)?;
    Ok(())
}
