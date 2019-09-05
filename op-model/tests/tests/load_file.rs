use super::*;
use kg_tree::opath::{FuncCallErrorDetail, FuncCallErrorDetail::*, FuncId, Opath, ScopeMut};
use op_model::LoadFileFunc;
use std::str::FromStr;

#[test]
fn non_existing_file() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    init_repo(&dir);
    let commit = initial_commit(&dir);

    let func = LoadFileFunc::new(dir.clone(), "".into(), commit);
    let scope = ScopeMut::new();
    scope.set_func("loadFile".into(), Box::new(func));
    let node = node!();

    let opath = Opath::parse("loadFile('some/path/to/file.yaml')").unwrap_disp();

    let res = opath.apply_one_ext(&node, &node, scope.as_ref());

    let (err, _detail) = assert_detail!(res, FuncCallErrorDetail, FuncCallCustom { id }, {
        assert_eq!(&FuncId::from("loadFile"), id);
    });
    assert_cause!(err);
}

#[test]
fn non_existing_repo() {
    let (_tmp, dir) = get_tmp_dir();

    let func = LoadFileFunc::new(dir.clone(), "".into(), Sha1Hash::nil());
    let scope = ScopeMut::new();
    scope.set_func("loadFile".into(), Box::new(func));
    let node = node!();

    let opath = Opath::parse("loadFile('some/path/to/file.yaml')").unwrap_disp();

    let res = opath.apply_one_ext(&node, &node, scope.as_ref());

    let (err, _detail) = assert_detail!(res, FuncCallErrorDetail, FuncCallCustom { id }, {
        assert_eq!(&FuncId::from("loadFile"), id);
    });
    assert_cause!(err);
}

#[test]
fn bad_args_num() {
    let (_tmp, dir) = get_tmp_dir();

    let func = LoadFileFunc::new(dir.clone(), "".into(), Sha1Hash::nil());
    let scope = ScopeMut::new();
    scope.set_func("loadFile".into(), Box::new(func));
    let node = node!();

    let opath = Opath::parse("loadFile(1, 2, 3)").unwrap_disp();

    let res = opath.apply_one_ext(&node, &node, scope.as_ref());

    let (_err, _detail) = assert_detail!(res, FuncCallErrorDetail, FuncCallInvalidArgCountRange{required_min, required_max, ..}, {
        assert_eq!(1, *required_min);
        assert_eq!(2, *required_max);
    });
}

#[test]
fn bad_commit_oid() {
    let (_tmp, dir) = get_tmp_dir();
    let dir = dir.join("model");
    init_repo(&dir);

    let func = LoadFileFunc::new(
        dir.clone(),
        "".into(),
        Sha1Hash::from_str("9306be9441bec94c673a494f05ffa389c1243d58").unwrap(),
    );
    let scope = ScopeMut::new();
    scope.set_func("loadFile".into(), Box::new(func));
    let node = node!();

    let opath = Opath::parse("loadFile('some/path/to/file.yaml')").unwrap_disp();

    let res = opath.apply_one_ext(&node, &node, scope.as_ref());

    let (err, _detail) = assert_detail!(res, FuncCallErrorDetail, FuncCallCustom { id }, {
        assert_eq!(&FuncId::from("loadFile"), id);
    });
    assert_cause!(err);
}

#[test]
fn arg_resolve_err() {
    let (_tmp, dir) = get_tmp_dir();

    let func = LoadFileFunc::new(dir.clone(), "".into(), Sha1Hash::nil());
    let scope = ScopeMut::new();
    scope.set_func("loadFile".into(), Box::new(func));
    let node = node!();

    let opath = Opath::parse("loadFile(nonExistingFunc())").unwrap_disp();

    let res = opath.apply_one_ext(&node, &node, scope.as_ref());

    let (err, _detail) = assert_detail!(res, FuncCallErrorDetail, FuncCallCustom { id }, {
        assert_eq!(&FuncId::from("loadFile"), id);
    });
    assert_cause!(err);
}

#[test]
fn single_param() {
    let (_tmp, dir) = get_tmp_dir();
    // language=json
    let content = r#"
{
  "key": "value"
}
"#;
    init_repo(&dir);
    write_file!(dir.join("example_file.json"), content);
    let commit = initial_commit(&dir);

    let func = LoadFileFunc::new(dir.clone(), "".into(), commit);
    let scope = ScopeMut::new();
    scope.set_func("loadFile".into(), Box::new(func));
    let node = node!();

    let opath = Opath::parse("loadFile('example_file' + '.json')").unwrap_disp();

    let res = opath
        .apply_one_ext(&node, &node, scope.as_ref())
        .unwrap_disp();

    assert_eq!("value", res.get_key("key").as_string_ext());
}

#[test]
fn two_params() {
    let (_tmp, dir) = get_tmp_dir();
    // language=json
    let content = r#"
{
  "key": "value"
}
"#;
    init_repo(&dir);
    write_file!(dir.join("example_file"), content);
    let commit = initial_commit(&dir);

    let func = LoadFileFunc::new(dir.clone(), "".into(), commit);
    let scope = ScopeMut::new();
    scope.set_func("loadFile".into(), Box::new(func));
    let node = node!();

    let opath = Opath::parse("loadFile('example_file', 'js' + 'on')").unwrap_disp();

    let res = opath
        .apply_one_ext(&node, &node, scope.as_ref())
        .unwrap_disp();

    assert_eq!("value", res.get_key("key").as_string_ext());
}

#[test]
fn node_parse_err() {
    let (_tmp, dir) = get_tmp_dir();
    // language=json
    let content = r#"
{
  "key": "value"
}
"#;
    init_repo(&dir);
    write_file!(dir.join("example_file.toml"), content);
    let commit = initial_commit(&dir);

    let func = LoadFileFunc::new(dir.clone(), "".into(), commit);
    let scope = ScopeMut::new();
    scope.set_func("loadFile".into(), Box::new(func));
    let node = node!();

    let opath = Opath::parse("loadFile('example_file.toml')").unwrap_disp();

    let res = opath.apply_one_ext(&node, &node, scope.as_ref());

    let (err, _detail) = assert_detail!(res, FuncCallErrorDetail, FuncCallCustom { id }, {
        assert_eq!(&FuncId::from("loadFile"), id);
    });
    assert_cause!(err);
}
