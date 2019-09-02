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

    let out = ctx.exec_op(&["query", "@.conf.hosts[*].@key"]);

    assert_out!(out);
    assert_eq!(expected, out.out);
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

    let out = ctx.exec_op(&["remote", "--", "ls -a"]);

    assert_out!(out);
    assert_eq!(expected, strip_ansi!(out.out));
}

#[test]
fn install_package() {
    let ctx = Context::new();

    // language=yaml
    let host = r#"
ssh_port: 8820
packages: [mc, vim-enhanced2, git] # append `git` package
ifaces:
  ens33:
    ip4: 127.0.0.1
"#;
    write_file!(ctx.model_dir().join("conf/hosts/zeus.yaml"), host);
    let out = ctx.exec_ssh("zeus", &["yum list installed git"]);
    assert_eq!(1, out.code);

    let out = ctx.exec_op(&["update"]);
    assert_out!(out);

    let out = ctx.exec_ssh("zeus", &["yum list installed git"]);
    assert_out!(out);
}

#[test]
fn remove_package() {
    let ctx = Context::new();

    // language=yaml
    let host = r#"
ssh_port: 8821
packages: [mc, vim-enhanced2] # remove `git` package
ifaces:
  ens33:
    ip4: 127.0.0.1
"#;
    write_file!(ctx.model_dir().join("conf/hosts/ares.yaml"), host);
    let out = ctx.exec_ssh("ares", &["yum install -y git"]);
    assert_out!(out);

    let out = ctx.exec_op(&["update"]);
    assert_out!(out);

    let out = ctx.exec_ssh("ares", &["yum list installed git"]);
    assert_eq!(1, out.code);
}

#[test]
fn update_etc_hosts() {
    let ctx = Context::new();

    let expected_etc_hosts = r#"127.0.0.1   localhost ares_renamed
::1         localhost ip6-localhost ip6-loopback
fe00::0     ip6-localnet
ff00::0     ip6-mcastprefix
ff02::1     ip6-allnodes
ff02::2     ip6-allrouters
"#;
    // remove yum procs. This should not be necessary after fix : https://github.com/opereon/opereon/issues/23
    remove_file!(ctx.model_dir().join("proc/yum/procs.yaml"));
    let out = ctx.exec_op(&["commit"]);
    assert_out!(out);
    let hosts_dir = ctx.model_dir().join("conf/hosts/");
    rename!(hosts_dir.join("ares.yaml"), hosts_dir.join("ares_renamed.yaml"));

    let out = ctx.exec_op(&["update"]);
    assert_out!(out);

    let out = ctx.exec_ssh("ares", &["cat /etc/hosts"]);
    assert_out!(out);
    assert_eq!(expected_etc_hosts, out.out);
}