use super::*;
use op_model::{TaskKind, Switch, Case };
use std::str::FromStr;
use kg_diag::BasicDiag;
use kg_tree::opath::Opath;

#[test]
fn proc_kind_from_str() {
    assert_eq!(TaskKind::Exec, TaskKind::from_str("exec").unwrap_disp());
    assert_eq!(TaskKind::Switch, TaskKind::from_str("switch").unwrap_disp());
    assert_eq!(TaskKind::Template, TaskKind::from_str("template").unwrap_disp());
    assert_eq!(TaskKind::Command, TaskKind::from_str("command").unwrap_disp());
    assert_eq!(TaskKind::Script, TaskKind::from_str("script").unwrap_disp());
    assert_eq!(TaskKind::FileCopy, TaskKind::from_str("file-copy").unwrap_disp());
    assert_eq!(TaskKind::FileCompare, TaskKind::from_str("file-compare").unwrap_disp());

    let res:Result<_, BasicDiag> = TaskKind::from_str("kitty").map_err(|err| {err.into()});
    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::UnknownTaskKind{value}, assert_eq!("kitty", value));
}

#[test]
fn case_parse() {
    // language=yaml
    let node = r#"
when: "${$missing_packages.length() > 0}"
run:
- tasks:
    - task: exec
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = Case::parse(&model, model.as_scoped(), &node).unwrap_disp();

    let opath = Opath::parse("$missing_packages.length() > 0").unwrap_disp();

    assert_eq!(&opath, def.when());
}

#[test]
fn case_parse_static_when() {
    // language=yaml
    let node = r#"
when: "static value"
run:
- tasks:
    - task: exec
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = Case::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::TaskCaseStaticWhen);
}

#[test]
fn case_parse_missing_when() {
    // language=yaml
    let node = r#"
run:
- tasks:
    - task: exec
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = Case::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::TaskCaseMissingWhen);
}

#[test]
fn case_parse_non_object() {
    // language=json
    let node = r#""string node""#;
    let node: NodeRef = node!(node, json);
    let model: Model = Model::empty();

    let res = Case::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::TaskCaseNonObject{kind}, assert_eq!(&Kind::String, kind));
    eprintln!("_err = {}", _err);
}

#[test]
fn switch_parse() {
    // language=yaml
    let node = r#"
- when: "${$missing_packages.length() > 0}"
  run:
    - tasks:
        - task: exec
- when: "${$missing_packages.length() > 0}"
  run:
    - tasks:
        - task: exec
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = Switch::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!(2, def.cases().len())
}


#[test]
fn switch_parse_non_array() {
    // language=yaml
    let node = r#"
some_prop: "aaa"
    "#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = Switch::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::TaskSwitchNonArray{kind}, assert_eq!(&Kind::Object, kind));
}