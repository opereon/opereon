use super::*;
use op_exec::{ModelManager, ConfigRef};
use std::fs::File;
use kg_diag::io::fs::{create_dir, read_string};

macro_rules! assert_exists {
    ($path: expr) => {
        let p = std::path::PathBuf::from($path);
        if !p.exists() {
            panic!("Path don't exist!: '{}'", p.display())
        }
    };
}

#[test]
fn search_manifest() {
    let (_tmp_dir, path) = get_tmp_dir();

    let manifest_path = path.join("op.toml");

    File::create(&manifest_path).unwrap();
    let found = ModelManager::search_manifest(&path).unwrap();

    assert_eq!(found, path);
}

#[test]
fn search_manifest_nested() {
    let (_tmp_dir, path) = get_tmp_dir();

    let manifest_path = path.join("op.toml");
    let nested_path = path.join("nested");

    File::create(&manifest_path).unwrap();

    let found = ModelManager::search_manifest(&nested_path).unwrap();

    assert_eq!(found, path);
}

#[test]
fn search_manifest_not_found() {
    let (_tmp_dir, path) = get_tmp_dir();

    let res = ModelManager::search_manifest(&path);

    let (_err, _detail) = assert_detail!(
        res,
        ModelErrorDetail,
        ModelErrorDetail::ManifestNotFound
    );
}

#[test]
fn init_model() {
    let (_tmp_dir, path) = get_tmp_dir();
    let cfg = ConfigRef::default();
    let manager = ModelManager::new(path.clone(), cfg, discard_logger!());

    manager.init_model(&path).unwrap();

    assert_exists!(path.join(".git"));
    assert_exists!(path.join(".operc"));
    assert_exists!(path.join("op.toml"));
}

#[test]
fn init_model_current() {
    let (_tmp_dir, path) = get_tmp_dir();
    let cfg = ConfigRef::default();
    let manager = ModelManager::new(path.clone(), cfg, discard_logger!());

    manager.init_model(&path).unwrap();

    assert_exists!(path.join(".git"));
    assert_exists!(path.join(".operc"));
    assert_exists!(path.join("op.toml"));
}

#[test]
fn init_model_do_not_override() {
    let (_tmp_dir, path) = get_tmp_dir();
    let cfg = ConfigRef::default();
    let manager = ModelManager::new(path.clone(), cfg, discard_logger!());

    let git_dir = path.join(".git");
    let config_dir = path.join(".operc");
    let manifest_dir = path.join("op.toml");


    create_dir(&git_dir).unwrap_disp();
    write_file!(config_dir, "content");
    write_file!(manifest_dir, "content");

    manager.init_model(&path).unwrap();

    assert_exists!(&git_dir);
    assert_exists!(&config_dir);
    assert_exists!(&manifest_dir);

    assert!(git_dir.read_dir().unwrap().next().is_none());

    assert_eq!("content", read_string(config_dir).unwrap_disp());
    assert_eq!("content", read_string(manifest_dir).unwrap_disp());
}