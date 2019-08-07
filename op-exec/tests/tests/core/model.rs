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

    let err = ModelManager::search_manifest(&path).unwrap_err();

    assert_err!(err.kind(), std::io::ErrorKind::NotFound)
}

#[test]
fn init_model() {
    let (_tmp_dir, path) = get_tmp_dir();

    ModelManager::init_model(&path).unwrap();

    assert_exists!(path.join(".git"));
    assert_exists!(path.join(".operc"));
    assert_exists!(path.join("op.toml"));
}

#[test]
fn init_model_current() {
    let (_tmp_dir, path) = get_tmp_dir();

    ModelManager::init_model(&path).unwrap();

    assert_exists!(path.join(".git"));
    assert_exists!(path.join(".operc"));
    assert_exists!(path.join("op.toml"));
}
