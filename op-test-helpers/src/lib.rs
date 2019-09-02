use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use std::fmt::Display;
pub use tempfile::TempDir;

mod git;
mod node;
pub use git::*;
pub use node::*;

pub use copy_dir;

#[macro_export]
macro_rules! write_file {
    ($path: expr, $content:expr) => {{
        use std::io::Write;
        let mut f = std::fs::File::create(&$path)
            .expect(&format!("Cannot create file: '{}'", $path.display()));
        f.write_all($content.as_ref())
            .expect(&format!("Cannot write file: '{}'", $path.display()))
    }};
}

#[macro_export]
macro_rules! remove_file {
    ($path: expr) => {{
        std::fs::remove_file($path).expect(&format!("Cannot remove file: '{}'", $path.display()))
    }};
}

#[macro_export]
macro_rules! rename {
    ($from: expr, $to: expr) => {{
        std::fs::rename(&$from, &$to).expect(&format!("Cannot rename file file: '{}'", $from.display()))
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
            err => panic!(
                "Expected error {} got {:?} : \n{}",
                stringify!($variant),
                err,
                err
            ),
        }
    }};
}

#[macro_export]
macro_rules! assert_cause {
    ($err: expr) => {{
        use kg_diag::Diag;
        $err.cause().expect("Missing cause!")
    }};
    ($err: expr, $detail:path) => {{
        use kg_diag::Diag;
        let cause = $err.cause().expect("Missing cause!");
        cause
            .detail()
            .downcast_ref::<$detail>()
            .expect(&format!("Cannot downcast to '{}'", stringify!($detail)))
    }};
}

/// Returns test resources directory located in `CARGO_MANIFEST_DIR/tests/resources/`.
#[macro_export]
macro_rules! resources_dir {
    () => {{
        let mut d = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        d.push("tests/resources");
        if !d.exists() {
            panic!("Resources dir not exists! {}", d.display())
        }
        d
    }};
}

/// Copy resources from `CARGO_MANIFEST_DIR/tests/resources/{resource}` to `target`.
#[macro_export]
macro_rules! copy_resource {
    ($resource: expr, $target:expr) => {{
        let r = resources_dir!().join(&$resource);
        let res = op_test_helpers::copy_dir::copy_dir(&r, &$target).expect(&format!(
            "Cannot copy test resource '{:?}' to '{:?}'",
            r, $target
        ));
        if !res.is_empty() {
            for err in res {
                eprintln!("err = {:?}", err);
            }
            panic!("Cannot copy test resource!")
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

/// Helper trait for `to_string` conversions
pub trait ToStringExt {
    fn to_string_ext(&self) -> String;
}

impl ToStringExt for &Path {
    fn to_string_ext(&self) -> String {
        self.to_str().unwrap().to_string()
    }
}

impl ToStringExt for PathBuf {
    fn to_string_ext(&self) -> String {
        self.to_str().unwrap().to_string()
    }
}

impl ToStringExt for Vec<u8> {
    fn to_string_ext(&self) -> String {
        String::from_utf8_lossy(&self).to_string()
    }
}
