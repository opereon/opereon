use super::*;
use op_test_helpers::{get_tmp_dir, TempDir};
use std::env::args;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

pub use common::*;
use std::time::Duration;

mod common;

#[test]
fn query_hosts() {
    let ctx = Context::new();

    let expected = r#"---
- ares
- zeus
"#;

    let (out, _, _) = ctx.exec_op(&["query", "@.conf.hosts[*].@key"]);

    assert_eq!(expected, out);
}

#[test]
fn remote_ls() {
    let ctx = Context::new();

    let expected = r#"---
- ares
- zeus
"#;

    std::thread::sleep(Duration::from_secs(3));

    let (out, err, code) = ctx.exec_op(&["remote", "--", "ls -al"]);

    eprintln!("out: {}\nerr: {}\ncode: {}", out, err, code);

//    assert_eq!(expected, out);
}
