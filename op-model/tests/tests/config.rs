use super::*;
use op_model::{Config, ConfigResolver};
use op_model::{GitErrorDetail, ModelErrorDetail, ModelErrorDetail::*};
use std::path::PathBuf;

#[test]
fn resolver_scan_revision() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    copy_resource!("model1", &dir);
    init_repo(&dir);
    let commit = initial_commit(&dir);

    let cr: ConfigResolver = ConfigResolver::scan_revision(&dir, &commit).unwrap_disp();

    let cfgs: Vec<(&PathBuf, &Config)> = cr.iter().collect();
    assert_eq!(4, cfgs.len());
    assert_eq!("", cfgs[0].0.to_string_lossy());
    assert_eq!("conf/hosts", cfgs[1].0.to_string_ext());
    assert_eq!("conf/users", cfgs[2].0.to_string_ext());
    assert_eq!("proc/hosts_file", cfgs[3].0.to_string_ext());
}

#[test]
fn resolver_scan_bad_git_path() {
    let (_tmp, dir) = get_tmp_dir();
    let res = ConfigResolver::scan_revision(&dir, &Oid::nil());

    let (_err, _detail) = assert_detail!(res, GitErrorDetail, GitErrorDetail::OpenRepository{..});
}

#[test]
fn resolver_scan_non_utf8() {
    let (_tmp, dir) = get_tmp_dir();
    write_file!(dir.join(".operc"), 0xFF8080_u32.to_be_bytes());
    init_repo(&dir);
    let commit = initial_commit(&dir);

    let res = ConfigResolver::scan_revision(&dir, &commit);
    let (_err, _detail) = assert_detail!(res, ModelErrorDetail, ConfigUtf8{..});
}

#[test]
fn resolver_malformed_config() {
    let (_tmp, dir) = get_tmp_dir();

    // language=toml
    let content = r#"
exclude="unexpected string"
"#;

    write_file!(dir.join(".operc"), content);
    init_repo(&dir);
    let commit = initial_commit(&dir);

    let res = ConfigResolver::scan_revision(&dir, &commit);
    let (_err, _detail) = assert_detail!(res, ModelErrorDetail, MalformedConfigFile{..});
}
