use super::*;
use kg_diag::BasicDiag;
use kg_tree::opath::Opath;
use kg_tree::FileFormat;
use op_model::{Case, OutputMode, Switch, TaskDef, TaskEnv, TaskKind, TaskOutput};
use std::str::FromStr;

#[test]
fn proc_kind_from_str() {
    assert_eq!(TaskKind::Exec, TaskKind::from_str("exec").unwrap_disp());
    assert_eq!(TaskKind::Switch, TaskKind::from_str("switch").unwrap_disp());
    assert_eq!(
        TaskKind::Template,
        TaskKind::from_str("template").unwrap_disp()
    );
    assert_eq!(
        TaskKind::Command,
        TaskKind::from_str("command").unwrap_disp()
    );
    assert_eq!(TaskKind::Script, TaskKind::from_str("script").unwrap_disp());
    assert_eq!(
        TaskKind::FileCopy,
        TaskKind::from_str("file-copy").unwrap_disp()
    );
    assert_eq!(
        TaskKind::FileCompare,
        TaskKind::from_str("file-compare").unwrap_disp()
    );

    let res: Result<_, BasicDiag> = TaskKind::from_str("kitty").map_err(|err| err.into());
    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::UnknownTaskKind { value },
        assert_eq!("kitty", value)
    );
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
fn case_parse_bad_whe_expression() {
    // language=yaml
    let node = r#"
when: "${@.@}"

"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = Case::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::PropParseErr{prop, ..}, assert_eq!("when", prop));
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

    let (_err, _detail) =
        assert_detail!(res, DefsErrorDetail, DefsErrorDetail::TaskCaseMissingWhen);
}

#[test]
fn case_parse_non_object() {
    // language=json
    let node = r#""string node""#;
    let node: NodeRef = node!(node, json);
    let model: Model = Model::empty();

    let res = Case::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::TaskCaseNonObject { kind },
        assert_eq!(&Kind::String, kind)
    );
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

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::TaskSwitchNonArray { kind },
        assert_eq!(&Kind::Object, kind)
    );
}

#[test]
fn task_env_parse_obj() {
    // language=yaml
    let node = r#"
variable1: "$.conf.hosts"
variable2: "@.some.path"
"#;
    let node: NodeRef = node!(node, yaml);

    let env = TaskEnv::parse(&node).unwrap_disp();

    match env {
        TaskEnv::Map(envs) => {
            assert_eq!("$.conf.hosts", envs.get("variable1").unwrap().to_string());
            assert_eq!("@.some.path", envs.get("variable2").unwrap().to_string());
        }
        TaskEnv::List(_) => panic!("Map expected"),
    }
}

#[test]
fn task_env_parse_array() {
    // language=yaml
    let node = r#"
- "$.conf.hosts"
- "@.some.path"
"#;
    let node: NodeRef = node!(node, yaml);

    let env = TaskEnv::parse(&node).unwrap_disp();

    match env {
        TaskEnv::Map(_) => panic!("List expected"),
        TaskEnv::List(envs) => {
            assert_eq!("$.conf.hosts", envs[0].to_string());
            assert_eq!("@.some.path", envs[1].to_string());
        }
    }
}

#[test]
fn task_env_parse_string() {
    // language=json
    let node = r#""${@.some.path}""#;
    let node: NodeRef = node!(node, json);

    let env = TaskEnv::parse(&node).unwrap_disp();

    match env {
        TaskEnv::Map(_) => panic!("List expected"),
        TaskEnv::List(envs) => {
            assert_eq!("@.some.path", envs[0].to_string());
        }
    }
}

#[test]
fn task_env_parse_obj_err() {
    // language=yaml
    let node = r#"
illegal_obj:
  nested: "val"
"#;
    let node: NodeRef = node!(node, yaml);

    let res = TaskEnv::parse(&node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::EnvPropParseErr{prop, ..}, assert_eq!("illegal_obj", prop));
}

#[test]
fn task_env_parse_array_err() {
    // language=yaml
    let node = r#"
- illegal_obj:
  nested: "val"
"#;
    let node: NodeRef = node!(node, yaml);

    let res = TaskEnv::parse(&node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::EnvPropParseErr{prop, ..}, assert_eq!("0", prop));
}

#[test]
fn task_env_parse_string_err() {
    // language=yaml
    let node = r#""${@.@}""#;
    let node: NodeRef = node!(node, yaml);

    let res = TaskEnv::parse(&node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::OpathParseErr{..});
}

#[test]
fn task_env_parse_illegal_type() {
    // language=json
    let node = r#"1234"#;
    let node: NodeRef = node!(node, json);

    let res = TaskEnv::parse(&node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::UnexpectedPropType { kind, expected },
        {
            assert_eq!(&Kind::Integer, kind);
            assert_eq!(&vec![Kind::Object, Kind::Array, Kind::String], expected);
        }
    );
}

#[test]
fn task_output_parse_obj() {
    // language=yaml
    let node = r#"
format: json
expr: "@.some.expr"
"#;
    let node: NodeRef = node!(node, yaml);

    let out = TaskOutput::parse(&node).unwrap_disp();

    match out.mode() {
        OutputMode::Var(_) => panic!("Expr expected"),
        OutputMode::Expr(opath) => assert_eq!("@.some.expr", opath.to_string()),
    }
    assert_eq!(FileFormat::Json, out.format());
}

#[test]
fn task_output_parse_string() {
    // language=yaml
    let node = r#""yaml""#;
    let node: NodeRef = node!(node, yaml);

    let out = TaskOutput::parse(&node).unwrap_disp();

    match out.mode() {
        OutputMode::Var(var) => {
            assert_eq!("output", var);
        }
        OutputMode::Expr(_) => panic!("Var expected"),
    }
    assert_eq!(FileFormat::Yaml, out.format());
}

#[test]
fn task_output_parse_illegal_type() {
    // language=json
    let node = r#"1234"#;
    let node: NodeRef = node!(node, json);

    let res = TaskOutput::parse(&node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::UnexpectedPropType { kind, expected },
        {
            assert_eq!(&Kind::Integer, kind);
            assert_eq!(&vec![Kind::Object, Kind::String], expected);
        }
    );
}

#[test]
fn task_def_command_parse() {
    // language=yaml
    let node = r#"
task: command
id: task-id
label: task-label
ro: true
env:
  var1: "Value1"
  var2: "Value2"

"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = TaskDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!("task-id", def.id());
    assert_eq!("task-label", def.label());
    assert_eq!(TaskKind::Command, def.kind());
    assert!(def.read_only());
    assert!(def.switch().is_none());
    assert!(def.output().is_none());
    assert!(def.env().is_some());
}

#[test]
fn task_def_switch_parse() {
    // language=yaml
    let node = r#"
task: switch
cases:
  - when: "${@}"
    key: val1
  - when: "${$}"
    key: val2
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = TaskDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!(TaskKind::Switch, def.kind());
    assert!(def.switch().is_some());
    let switch = def.switch().unwrap();
    assert_eq!(2, switch.cases().len())
}

#[test]
fn task_def_script_output() {
    // language=yaml
    let node = r#"
task: script
output:
    format: json
    expr: "@.some.expr"
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = TaskDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!(TaskKind::Script, def.kind());
    assert!(def.output().is_some());
}

#[test]
fn task_def_missing_task() {
    // language=yaml
    let node = r#"
output:
    format: json
    expr: "@.some.expr"
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = TaskDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::TaskMissingTask);
}

#[test]
fn task_def_env_parse_err() {
    // language=yaml
    let node = r#"
task: command
env: 1234

"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = TaskDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::EnvParseErr{..});
}

#[test]
fn task_def_switch_parse_err() {
    // language=yaml
    let node = r#"
task: switch
cases:
  - when: "static prop"
    key: val1
  - when: "${$}"
    key: val2
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = TaskDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::SwitchParseErr{..});
}

#[test]
fn task_def_output_parse_err() {
    // language=yaml
    let node = r#"
task: command
output: 1234
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = TaskDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::OutputParseErr{..});
}

#[test]
fn task_def_unexpected_type() {
    // language=yaml
    let node = r#""unexpected string""#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = TaskDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::UnexpectedPropType { kind, expected },
        {
            assert_eq!(&Kind::String, kind);
            assert_eq!(&vec![Kind::Object], expected);
        }
    );
}
