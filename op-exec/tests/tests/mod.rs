use super::*;
use std::ffi::OsStr;
use std::path::PathBuf;
use tempfile::TempDir;

macro_rules! write_file {
    ($path: expr, $content:expr) => {{
        let mut f = std::fs::File::create(&$path)
            .expect(&format!("Cannot create file: '{}'", $path.display()));
        f.write_all($content.as_bytes())
            .expect(&format!("Cannot write file: '{}'", $path.display()))
    }};
}

macro_rules! assert_detail {
    ($res: expr, $detail:ident, $variant: pat) => {{
        let err = match $res {
            Ok(ref val) => panic!("Error expected, got {:?}", val),
            Err(ref err) => err,
        };
        let det = err
            .detail()
            .downcast_ref::<$detail>()
            .expect(&format!("Cannot downcast to '{}'", stringify!($detail)));

        match det {
            $variant => (err, det),
            err => panic!("Expected error {} got {:?}", stringify!($variant), err),
        }
    }};
}

/// Get absolute path to the "target" directory ("build" dir)
pub fn get_target_dir() -> PathBuf {
    let bin = std::env::current_exe().expect("exe path");
    let mut target_dir = PathBuf::from(bin.parent().expect("bin parent"));
    while target_dir.file_name() != Some(OsStr::new("target")) {
        target_dir.pop();
    }
    target_dir
}

/// Get temporary directory located in "target"
pub fn get_tmp_dir() -> TempDir {
    let target = get_target_dir();
    let resources_dir = target.join("test_resources");

    if let Err(err) = std::fs::create_dir(&resources_dir) {
        if err.kind() != std::io::ErrorKind::AlreadyExists {
            panic!("Cannot create test resources dir: {:?}", err)
        }
    }
    tempfile::tempdir_in(resources_dir).expect("Cannot create temporary dir!")
}

mod config;
mod core;
