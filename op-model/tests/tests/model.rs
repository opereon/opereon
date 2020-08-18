use super::*;
use kg_diag::IoErrorDetail;
use kg_tree::opath::FuncCallErrorDetail;
use op_model::DefsErrorDetail;
use op_model::{ Model, ModelErrorDetail, ModelErrorDetail::*};
use op_test_helpers::{get_tmp_dir, UnwrapDisplay, init_repo};
use op_rev::RevInfo;


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

    assert_eq!(
        &["author@example.com".to_string()],
        manifest.info().authors()
    );
    assert_eq!("Opereon model", manifest.info().description());

    assert_eq!("$.users", manifest.defines().users().to_string());
    assert_eq!("$.hosts", manifest.defines().hosts().to_string());
    assert_eq!("$.(\"proc\", \"probe\")", manifest.defines().procs().to_string());
    assert_eq!(
        "@.custom.expr",
        manifest.defines().custom()["custom_prop"].to_string()
    );
    //    assert_eq!(false, manifest.defines().is_user_defined());
}

#[test]
fn load_manifest_utf8_err() {
    let (_tmp, dir) = get_tmp_dir();
    // non-utf8 character
    let content = 0xE08080_u32.to_be_bytes();
    write_file!(dir.join("op.toml"), content);

    let res = Model::load_manifest(&dir);

    let (err, _detail) = assert_detail!(res, ModelErrorDetail, ManifestRead{..});
    let _cause = assert_cause!(err, IoErrorDetail);
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

    let (err, _detail) = assert_detail!(res, ModelErrorDetail, ManifestParse{..});
    let _cause = assert_cause!(err, kg_tree::serial::Error);
}

#[test]
fn read_include_item_expr_err() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    copy_resource!("model1", &dir);
    init_repo(&dir);
    // language=toml
    let content = r#"
inherit_includes = false
[overrides]
"@.*" = "@.extend(loadFile('conf/users/_default.yaml'), 0)"

[[exclude]]
path = "_default.*"

[[include]]
path = "**/example.yaml"
file_type = "file"
item = "unknownFunc()"
mapping = "$.find(array($item.@file_path_components[:-2]).join('.', '\"')).extend($item)"
"#;
    write_file!(dir.join("conf/users/.operc"), content);
    let commit = initial_commit(&dir);

    let rev_info = RevInfo::new(commit, dir.clone());

    let res = Model::read(rev_info);

    let (err, _detail) = assert_detail!(res, ModelErrorDetail, IncludesResolve{..});

    let _cause = assert_cause!(err, ModelErrorDetail);
}

#[test]
fn read_include_mapping_expr_err() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    copy_resource!("model1", &dir);
    init_repo(&dir);
    // language=toml
    let content = r#"
inherit_includes = false
[overrides]
"@.*" = "@.extend(loadFile('conf/users/_default.yaml'), 0)"

[[exclude]]
path = "_default.*"

[[include]]
path = "**/example.yaml"
file_type = "file"
item = "loadFile(@file_path, @file_ext)"
mapping = "unknownFunc()"
"#;
    write_file!(dir.join("conf/users/.operc"), content);
    let commit = initial_commit(&dir);
    let rev_info = RevInfo::new(commit, dir.clone());

    let res = Model::read(rev_info);

    let (err, _detail) = assert_detail!(res, ModelErrorDetail, IncludesResolve{..});
    let _cause = assert_cause!(err, ModelErrorDetail);
}

#[test]
fn read_override_val_expr_err() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    copy_resource!("model1", &dir);
    init_repo(&dir);
    // language=toml
    let content = r#"
[overrides]
"@.*" = "@.extend(unknownFunc(), 0)"

[[exclude]]
path = "_default.*"
"#;
    write_file!(dir.join("conf/users/.operc"), content);
    let commit = initial_commit(&dir);
    let rev_info = RevInfo::new(commit, dir.clone());

    let res = Model::read(rev_info);

    let (err, _detail) = assert_detail!(res, ModelErrorDetail, OverridesResolve{..});
    let _cause = assert_cause!(err, ModelErrorDetail);
}

#[test]
fn read_override_key_expr_err() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    copy_resource!("model1", &dir);
    init_repo(&dir);
    // language=toml
    let content = r#"
[overrides]
"unknownFunc(@)" = "@.extend(loadFile('conf/users/_default.yaml'), 0)"

[[exclude]]
path = "_default.*"
"#;
    write_file!(dir.join("conf/users/.operc"), content);
    let commit = initial_commit(&dir);
    let rev_info = RevInfo::new(commit, dir.clone());

    let res = Model::read(rev_info);

    let (err, _detail) = assert_detail!(res, ModelErrorDetail, OverridesResolve{..});
    let _cause = assert_cause!(err, ModelErrorDetail);
}

#[test]
fn read_interpolation_err() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    copy_resource!("model1", &dir);
    init_repo(&dir);
    // language=yaml
    let content = r#"
username: <% unknownFunc(@) %>
"#;
    write_file!(dir.join("conf/users/_default.yaml"), content);
    let commit = initial_commit(&dir);
    let rev_info = RevInfo::new(commit, dir.clone());

    let res = Model::read(rev_info);

    let (err, _detail) = assert_detail!(res, ModelErrorDetail, InterpolationsResolve);
    let _cause = assert_cause!(err, FuncCallErrorDetail);
}

#[test]
fn read_host_def_parse_err() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    copy_resource!("model1", &dir);
    init_repo(&dir);
    // language=yaml
    let content = r#"
hostname: some.hostname
#ssh_dest:
#  port: 22
#  username: root
#  auth:
#    method: public-key
#    identity_file: ~/.ssh/id_rsa
"#;
    remove_file!(dir.join("conf/hosts/.operc"));
    remove_file!(dir.join("conf/hosts/_default.yaml"));
    write_file!(dir.join("conf/hosts/fedora.yaml"), content);
    let commit = initial_commit(&dir);
    let rev_info = RevInfo::new(commit, dir.clone());

    let res = Model::read(rev_info);

    let (err, _detail) = assert_detail!(res, ModelErrorDetail, DefsParse{..});
    let _cause = assert_cause!(err, DefsErrorDetail);
}

#[test]
fn read_user_def_parse_err() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    copy_resource!("model1", &dir);
    init_repo(&dir);
    // language=yaml
    let content = r#"
# username: example_user
some_prop: "value"
"#;
    remove_file!(dir.join("conf/users/.operc"));
    remove_file!(dir.join("conf/users/_default.yaml"));
    remove_file!(dir.join("conf/users/example2.toml"));
    write_file!(dir.join("conf/users/example.yaml"), content);
    let commit = initial_commit(&dir);
    let rev_info = RevInfo::new(commit, dir.clone());

    let res = Model::read(rev_info);

    let (err, _detail) = assert_detail!(res, ModelErrorDetail, DefsParse{..});
    let _cause = assert_cause!(err, DefsErrorDetail);
}

#[test]
fn read_proc_def_parse_err() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    copy_resource!("model1", &dir);
    init_repo(&dir);
    // language=yaml
    let content = r#"
updates:
  proc: unknown_proc_kind
  label: update /etc/hosts
"#;
    write_file!(dir.join("proc/hosts_file/_.yaml"), content);
    let commit = initial_commit(&dir);
    let rev_info = RevInfo::new(commit, dir.clone());

    let res = Model::read(rev_info);

    let (err, _detail) = assert_detail!(res, ModelErrorDetail, DefsParse{..});
    let _cause = assert_cause!(err, DefsErrorDetail);
}

#[test]
fn read() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    copy_resource!("model1", &dir);
    init_repo(&dir);
    let commit = initial_commit(&dir);
    let rev_info = RevInfo::new(commit, dir.clone());

    let model = Model::read(rev_info).unwrap_disp();

    assert_eq!(1, model.hosts().len());
    assert_eq!("fedora.domain.com", model.hosts()[0].hostname());

    assert_eq!(2, model.users().len());
    assert_eq!("example", model.users()[0].username());
    assert_eq!("example2", model.users()[1].username());
}
