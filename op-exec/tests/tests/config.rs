use super::*;
use op_exec::{ConfigErrorDetail, ConfigRef, LogLevel};

const MAIN_CFG: &str = r##"
    run_dir = "/var/run/opereon1"
    data_dir = "/var/lib/opereon1"

    [daemon]
    socket_path = "/var/run/opereon1/op.sock"
    pid_file_path = "/var/run/opereon1/op.pid"

    [log]
    level = "warning"
    log_path = "/var/log/opereon1/opereon.log"

    "##;

const SECOND_CFG: &str = r##"
    [queue]
    persist_dir = "/var/run/opereon1/queue"
    "##;

#[test]
fn read_single_config() {
    let (_tmp_dir, path) = get_tmp_dir();

    let cfg_path = path.join("config.toml");

    write_file!(cfg_path, MAIN_CFG);

    let cfg = ConfigRef::read(cfg_path.to_str().unwrap()).expect("Cannot read config");

    assert_eq!("/var/run/opereon1", cfg.run_dir().to_str().unwrap());
    assert_eq!("/var/lib/opereon1", cfg.data_dir().to_str().unwrap());

    assert_eq!(
        "/var/run/opereon1/op.sock",
        cfg.daemon().socket_path().to_str().unwrap()
    );
    assert_eq!(
        "/var/run/opereon1/op.pid",
        cfg.daemon().pid_file_path().to_str().unwrap()
    );

    assert_eq!(&LogLevel::Warning, cfg.log().level());
    assert_eq!(
        "/var/log/opereon1/opereon.log",
        cfg.log().log_path().to_str().unwrap()
    );
}

#[test]
fn read_multiple_configs() {
    let (_tmp_dir, path) = get_tmp_dir();

    let cfg1_path = path.join("config1.toml");
    let cfg2_path = path.join("config2.toml");

    write_file!(cfg1_path, MAIN_CFG);
    write_file!(cfg2_path, SECOND_CFG);

    let paths = format!("{};{}", cfg1_path.display(), cfg2_path.display());

    let cfg = ConfigRef::read(&paths).expect("Cannot read config");

    assert_eq!("/var/run/opereon1", cfg.run_dir().to_str().unwrap());
    assert_eq!("/var/lib/opereon1", cfg.data_dir().to_str().unwrap());

    assert_eq!(
        "/var/run/opereon1/op.sock",
        cfg.daemon().socket_path().to_str().unwrap()
    );
    assert_eq!(
        "/var/run/opereon1/op.pid",
        cfg.daemon().pid_file_path().to_str().unwrap()
    );

    assert_eq!(&LogLevel::Warning, cfg.log().level());
    assert_eq!(
        "/var/log/opereon1/opereon.log",
        cfg.log().log_path().to_str().unwrap()
    );

    assert_eq!(
        "/var/run/opereon1/queue",
        cfg.queue().persist_dir().to_str().unwrap()
    );
}

#[test]
fn read_with_interpolations() {
    let (_tmp_dir, path) = get_tmp_dir();

    let cfg_path = path.join("config.toml");

    let cfg = r##"
        run_dir = "${'some_expression' + 2}"
    "##;

    write_file!(cfg_path, cfg);

    let cfg = ConfigRef::read(cfg_path.to_str().unwrap()).expect("Cannot read config");

    assert_eq!("some_expression2", cfg.run_dir().to_str().unwrap());
}

#[test]
fn from_json() {
    let json = r##"
    {
        "queue": {
            "persist_dir": "/var/run/opereon1/queue"
        }
    }
    "##;

    let cfg = ConfigRef::from_json(json).expect("Cannot parse json config");

    assert_eq!(
        "/var/run/opereon1/queue",
        cfg.queue().persist_dir().to_str().unwrap()
    );
}

#[test]
fn config_not_found() {
    let (_tmp_dir, path) = get_tmp_dir();

    let cfg_path = path.join("config.toml");

    let res = ConfigRef::read(cfg_path.to_str().unwrap());

    assert_detail!(res, ConfigErrorDetail, ConfigErrorDetail::NotFound {..});
}

#[test]
fn file_config_parse_err() {
    let (_tmp_dir, path) = get_tmp_dir();

    let cfg_path = path.join("config.toml");

    let cfg = r#"
        maflormed = %
        run_dir = "/var/run/opereon1"
    "#;

    write_file!(cfg_path, cfg);

    let res = ConfigRef::read(cfg_path.to_str().unwrap());

    assert_detail!(res, ConfigErrorDetail, ConfigErrorDetail::ParseFileErr {..});
}

#[test]
fn json_config_parse_err() {
    let json = r##"
    {
        "queue": {
            "persist_dir": &&
        }
    }
    "##;

    let res = ConfigRef::from_json(json);

    assert_detail!(res, ConfigErrorDetail, ConfigErrorDetail::ParseErr {..});
}
