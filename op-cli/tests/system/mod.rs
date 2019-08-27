use super::*;
use op_test_helpers::{get_tmp_dir, TempDir};
use std::env::args;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

pub use common::*;
use std::time::Duration;

#[macro_use]
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

    let expected = r#"Info: Finished executing command on remote hosts!
Info: ================Host [ares.example.com]================
[ares.example.com] out: .
[ares.example.com] out: ..
[ares.example.com] out: .bash_logout
[ares.example.com] out: .bash_profile
[ares.example.com] out: .bashrc
[ares.example.com] out: .cshrc
[ares.example.com] out: .pki
[ares.example.com] out: .ssh
[ares.example.com] out: .tcshrc
Info: ================Host [zeus.example.com]================
[zeus.example.com] out: .
[zeus.example.com] out: ..
[zeus.example.com] out: .bash_logout
[zeus.example.com] out: .bash_profile
[zeus.example.com] out: .bashrc
[zeus.example.com] out: .cshrc
[zeus.example.com] out: .pki
[zeus.example.com] out: .ssh
[zeus.example.com] out: .tcshrc
"#;

    let (out, _err, _code) = ctx.exec_op(&["remote", "--", "ls -a"]);

    assert_eq!(expected, strip_ansi!(out));
}