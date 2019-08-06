use super::*;
use op_exec::ModelManager;
use std::fs::File;

macro_rules! assert_err {
    ($err: expr, $variant: pat) => {
        match $err {
            $variant => {}
            err => panic!("Expected error {} got {:?}", stringify!($variant), err),
        }
    };
}

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
    let tmp_dir = get_tmp_dir();
    let path = tmp_dir.path().to_path_buf();

    let manifest_path = path.join("op.toml");

    File::create(&manifest_path).unwrap();
    let found = ModelManager::search_manifest(&path).unwrap();

    assert_eq!(found, path);
}

#[test]
fn search_manifest_nested() {
    let tmp_dir = get_tmp_dir();
    let path = tmp_dir.path().to_path_buf();

    let manifest_path = path.join("op.toml");
    let nested_path = path.join("nested");

    File::create(&manifest_path).unwrap();

    let found = ModelManager::search_manifest(&nested_path).unwrap();

    assert_eq!(found, path);
}

#[test]
fn search_manifest_not_found() {
    let tmp_dir = get_tmp_dir();
    let path = tmp_dir.path().to_path_buf();

    let err = ModelManager::search_manifest(&path).unwrap_err();

    assert_err!(err.kind(), std::io::ErrorKind::NotFound)
}

#[test]
fn init_model() {
    let tmp_dir = get_tmp_dir();
    let path = tmp_dir.path().to_path_buf();

    ModelManager::init_model(&path).unwrap();

    assert_exists!(path.join(".git"));
    assert_exists!(path.join(".operc"));
    assert_exists!(path.join("op.toml"));
}

#[test]
fn init_model_current() {
    let tmp_dir = get_tmp_dir();
    let path = tmp_dir.path().to_path_buf();

    ModelManager::init_model(&path).unwrap();

    assert_exists!(path.join(".git"));
    assert_exists!(path.join(".operc"));
    assert_exists!(path.join("op.toml"));
}
