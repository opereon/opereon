use super::*;
use kg_diag::{BasicDiag, FileType};
use kg_tree::FileInfo;
use op_model::{ProcDef, ProcKind};
use std::path::PathBuf;
use std::str::FromStr;

#[test]
fn proc_kind_from_str() {
    assert_eq!(ProcKind::Exec, ProcKind::from_str("exec").unwrap_disp());
    assert_eq!(ProcKind::Check, ProcKind::from_str("check").unwrap_disp());
    assert_eq!(ProcKind::Update, ProcKind::from_str("update").unwrap_disp());
    assert_eq!(ProcKind::Probe, ProcKind::from_str("probe").unwrap_disp());

    let res: Result<_, BasicDiag> = ProcKind::from_str("kitty").map_err(|err| err.into());
    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::UnknownProcKind { value },
        assert_eq!("kitty", value)
    );
}

#[test]
fn proc_def_update() {
    // language=yaml
    let node = r#"
proc: update
id: proc-id
label: proc-label
watch:
  $.conf.hosts: +
watch_file:
  conf/hosts/**: +
run:
  step_0:
    tasks:
      - task: script
  step_1:
    tasks:
      - task: command
"#;
    let node: NodeRef = node!(node, yaml);
    let mut path = PathBuf::from("/path/to/source/file.yaml");
    let f = FileInfo::new(&path, FileType::File, "yaml".into());
    node.data_mut().set_file(Some(&f));
    let model: Model = Model::empty();

    let def = ProcDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!("proc-id", def.id());
    assert_eq!("proc-label", def.label());
    assert_eq!(&path, def.path());
    path.pop();
    assert_eq!(path, def.dir());
    assert_eq!(ProcKind::Update, def.kind());
    assert_eq!(1, def.model_watches().len());
    assert_eq!(1, def.file_watches().len());
    assert_eq!(2, def.run().steps().len());
}

#[test]
fn proc_def_watch_nulls() {
    // language=yaml
    let node = r#"
proc: update
id: proc-id
label: proc-label
watch: null
watch_file: null
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = ProcDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!(0, def.model_watches().len());
    assert_eq!(0, def.file_watches().len());
}

#[test]
fn proc_def_when_exec_type() {
    // language=yaml
    let node = r#"
when: "@.some.expr"
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let def = ProcDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!(ProcKind::Exec, def.kind());
}

#[test]
fn proc_def_model_watch_parse_err() {
    // language=yaml
    let node = r#"
proc: update
watch:
  "@.@": +
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = ProcDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) =
        assert_detail!(res, DefsErrorDetail, DefsErrorDetail::ProcModelWatchParseErr{..});
}

#[test]
fn proc_def_file_watch_parse_err() {
    // language=yaml
    let node = r#"
proc: update
watch_file:
  "[Z-A]": +
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = ProcDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) =
        assert_detail!(res, DefsErrorDetail, DefsErrorDetail::ProcFileWatchParseErr{..});
}

#[test]
fn proc_def_run_parse_err() {
    // language=yaml
    let node = r#"
proc: update
watch:
  $.conf.hosts: +
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

    let res = ProcDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::RunParseErr{..});
}

#[test]
fn proc_def_missing_proc() {
    // language=yaml
    let node = r#"
watch:
  $.conf.hosts: +
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

    let res = ProcDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::ProcMissingProc);
}

#[test]
fn proc_def_bad_proc_kind() {
    // language=yaml
    let node = r#"
proc: lala
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = ProcDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::UnknownProcKind { value },
        assert_eq!("lala", value)
    );
}

#[test]
fn proc_def_model_watch_non_obj() {
    // language=yaml
    let node = r#"
proc: update
watch: some-string
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = ProcDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::ProcWatchNonObject { kind },
        assert_eq!(&Kind::String, kind)
    );
}

#[test]
fn proc_def_file_watch_non_obj() {
    // language=yaml
    let node = r#"
proc: update
watch_file: some-string
"#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = ProcDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::ProcWatchNonObject { kind },
        assert_eq!(&Kind::String, kind)
    );
}

#[test]
fn proc_def_unexpected_type() {
    // language=yaml
    let node = r#""some string""#;
    let node: NodeRef = node!(node, yaml);
    let model: Model = Model::empty();

    let res = ProcDef::parse(&model, model.as_scoped(), &node);

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
