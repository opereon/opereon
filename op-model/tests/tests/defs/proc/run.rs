use super::*;
use op_model::{RunDef, Step};
use op_test_helpers::UnwrapDisplay;

#[test]
fn step_parse_array_tasks() {
    // language=yaml
    let node = r#"
hosts: "${@.conf.hosts}"
tasks:
  - task: command
    custom_prop: value
  - task: script
    custom_prop: value
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = Step::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!(2, def.tasks().len());
    assert!(def.hosts().is_some());
}

#[test]
fn step_parse_obj_tasks() {
    // language=yaml
    let node = r#"
tasks:
    task_1:
      task: command
      custom_prop: value
    task_2:
      task: script
      custom_prop: value
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = Step::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!(2, def.tasks().len());
    assert!(def.hosts().is_none());
}

#[test]
fn step_parse_static_hosts() {
    // language=yaml
    let node = r#"
hosts: "static value"
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = Step::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::StepStaticHosts);
}

#[test]
fn step_array_tasks_parse_err() {
    // language=yaml
    let node = r#"
tasks:
  - task_aa: command
    custom_prop: value
  - task: script
    custom_prop: value
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = Step::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::TaskMissingTask);
}

#[test]
fn step_obj_tasks_parse_err() {
    // language=yaml
    let node = r#"
hosts: "${@.conf.hosts}"
tasks:
    task_1:
      task_aa: command
      custom_prop: value
    task_2:
      task: script
      custom_prop: value
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = Step::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::TaskMissingTask);
}

#[test]
fn step_invalid_tasks_type() {
    // language=yaml
    let node = r#"
tasks: "string"
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = Step::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::UnexpectedPropType { kind, expected },
        {
            assert_eq!(&Kind::String, kind);
            assert_eq!(&vec![Kind::Array, Kind::Object], expected);
        }
    );
}

#[test]
fn step_parse_invalid_type() {
    // language=yaml
    let node = r#""Some string""#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = Step::parse(&model, model.as_scoped(), &node);

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

#[test]
fn step_parse_missing_tasks() {
    // language=yaml
    let node = r#"
some_prop: aaaaa
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = Step::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::StepMissingTasks);
}

#[test]
fn run_parse_obj() {
    // language=yaml
    let node = r#"
run:
  step_1:
    tasks:
      - task: script
  step_2:
    tasks:
      - task: command
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = RunDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!(2, def.steps().len())
}

#[test]
fn run_parse_arr() {
    // language=yaml
    let node = r#"
run:
  - tasks:
      - task: script
  - tasks:
      - task: command
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = RunDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!(2, def.steps().len())
}

#[test]
fn run_parse_null() {
    // language=yaml
    let node = r#"null"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = RunDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!(0, def.steps().len())
}

#[test]
fn run_parse_obj_step_parse_err() {
    // language=yaml
    let node = r#"
run:
  step_0:
    tasks:
      - task: script
  step_1:
    tasks_aaa:
      - task: command
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = RunDef::parse(&model, model.as_scoped(), &node);

    let (err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::StepParse{step, ..}, assert_eq!("1 (step_1)", step));
    assert_cause!(err);
}

#[test]
fn run_parse_array_step_parse_err() {
    // language=yaml
    let node = r#"
run:
  - tasks:
      - task: script
  - tasks_aa:
      - task: command
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = RunDef::parse(&model, model.as_scoped(), &node);

    let (err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::StepParse{step, ..}, assert_eq!("1", step));
    assert_cause!(err);
}

#[test]
fn run_parse_run_illegal_type() {
    // language=yaml
    let node = r#"
run: "some string"
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = RunDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::UnexpectedPropType { kind, expected },
        {
            assert_eq!(&Kind::String, kind);
            assert_eq!(&vec![Kind::Array, Kind::Object], expected);
        }
    );
}
