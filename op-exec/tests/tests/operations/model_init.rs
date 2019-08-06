use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use uuid::Uuid;

// Get absolute path to the "target" directory ("build" dir)
fn get_target_dir() -> PathBuf {
    let bin = std::env::current_exe().expect("exe path");
    let mut target_dir = PathBuf::from(bin.parent().expect("bin parent"));
    while target_dir.file_name() != Some(OsStr::new("target")) {
        target_dir.pop();
    }
    target_dir
}
// Get absolute path to the project's top dir, given target dir
fn get_top_dir(target_dir: &Path) -> &Path {
    target_dir.parent().expect("target parent")
}

fn get_tmp_dir() -> TempDir {
    let target = get_target_dir();
    let resources_dir = target.join("test_resources");

    if let Err(err) = std::fs::create_dir(&resources_dir) {
        if err.kind() != std::io::ErrorKind::AlreadyExists {
            panic!("Cannot create test resources dir: {:?}", err)
        }
    }
    tempfile::tempdir_in(resources_dir).expect("Cannot create temporary dir!")
}

#[test]
fn test() {
    let var = get_tmp_dir();

    eprintln!("var = {:?}", var.path());

    //    panic!()
}
