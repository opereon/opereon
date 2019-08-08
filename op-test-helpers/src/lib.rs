use std::ffi::OsStr;
use std::path::PathBuf;

use std::fmt::Display;
use tempfile::TempDir;

mod node;
pub use node::*;

#[macro_export]
macro_rules! write_file {
    ($path: expr, $content:expr) => {{
        use std::io::Write;
        let mut f = std::fs::File::create(&$path)
            .expect(&format!("Cannot create file: '{}'", $path.display()));
        f.write_all($content.as_bytes())
            .expect(&format!("Cannot write file: '{}'", $path.display()))
    }};
}

#[macro_export]
macro_rules! assert_detail {
    ($res: expr, $detail:ident, $variant: pat) => {
        assert_detail!($res, $detail, $variant, {})
    };
    ($res: expr, $detail:ident, $variant: pat, $block:expr) => {{
        use kg_diag::Diag;
        let err = match $res {
            Ok(ref val) => panic!("Error expected, got {:?}", val),
            Err(ref err) => err,
        };
        let det = err
            .detail()
            .downcast_ref::<$detail>()
            .expect(&format!("Cannot downcast to '{}'", stringify!($detail)));

        match det {
            $variant => {
                $block;
                (err, det)
            }
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

/// Create temporary directory located in "target/test_resources"
/// Returns handle to created directory
pub fn get_tmp_dir() -> (TempDir, PathBuf) {
    let target = get_target_dir();
    let resources_dir = target.join("test_resources");

    if let Err(err) = std::fs::create_dir(&resources_dir) {
        if err.kind() != std::io::ErrorKind::AlreadyExists {
            panic!("Cannot create test resources dir: {:?}", err)
        }
    }
    let dir = tempfile::tempdir_in(resources_dir).expect("Cannot create temporary dir!");
    let path = dir.path().to_path_buf();
    (dir, path)
}

/// Helper trait for pretty displaying error messages
pub trait UnwrapDisplay<T> {
    /// Same as `.unwrap()` but uses `Display` instead of `Debug`.
    fn unwrap_disp(self) -> T;
}

impl<T, E> UnwrapDisplay<T> for Result<T, E>
where
    E: Display,
{
    fn unwrap_disp(self) -> T {
        match self {
            Ok(val) => val,
            Err(err) => panic!("called `Result::unwrap()` on an `Err`:\n{}", err),
        }
    }
}
