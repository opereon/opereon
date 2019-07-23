use std::process::Command;

fn main() {
    let version = String::from_utf8(
        Command::new("git")
            .arg("describe")
            .arg("--long")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();
    let timestamp = String::from_utf8(
        Command::new("git")
            .arg("log")
            .arg("-1")
            .arg("--format=%cd")
            .arg("--date=iso-strict")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap();

    println!(
        "cargo:rustc-env=OP_SHORT_VERSION=v{}",
        env!("CARGO_PKG_VERSION")
    );
    println!("cargo:rustc-env=OP_LONG_VERSION={}", &version);
    println!("cargo:rustc-env=OP_TIMESTAMP={}", &timestamp);
}
