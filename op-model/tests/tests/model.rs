use super::*;
use op_model::{Model, ModelErrorDetail};
use kg_diag::io::IoError;

#[test]
fn load_manifest() {
    let (_tmp, dir) = get_tmp_dir();
    // language=toml
    let content = r#"
[info]
authors = ["author@example.com"]
description = "Opereon model"

[defines]
users = "$.users"
hosts = "$.hosts"
procs = "$.(proc, probe)"
custom_prop = "@.custom.expr"

"#;
    write_file!(dir.join("op.toml"), content);

    let manifest = Model::load_manifest(&dir).unwrap_disp();

    assert_eq!(&["author@example.com".to_string()], manifest.info().authors());
    assert_eq!("Opereon model", manifest.info().description());

    assert_eq!("$.users", manifest.defines().users().to_string());
    assert_eq!("$.hosts", manifest.defines().hosts().to_string());
    assert_eq!("$.(proc, probe)", manifest.defines().procs().to_string());
    assert_eq!("@.custom.expr", manifest.defines().custom()["custom_prop"].to_string());
//    assert_eq!(false, manifest.defines().is_user_defined());
}

#[test]
fn load_manifest_utf8_err() {
    let (_tmp, dir) = get_tmp_dir();
    // non-utf8 character
    let content = 0xE08080_u32.to_be_bytes();
    write_file!(dir.join("op.toml"), content);

    let res = Model::load_manifest(&dir);

    let (_err, _detail) = assert_detail!(res, IoError, IoError::IoPath{..});
}

#[test]
fn load_manifest_malformed_manifest() {
    let (_tmp, dir) = get_tmp_dir();
    // language=toml
    let content = r#"
info = "unexpected string"
"#;
    write_file!(dir.join("op.toml"), content);
    let res = Model::load_manifest(&dir);

    let (_err, _detail) = assert_detail!(res, ModelErrorDetail, ModelErrorDetail::MalformedManifest{..});
}

