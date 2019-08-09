use super::*;
use kg_diag::BasicDiag;
use op_model::{ProcDef, ProcKind};
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

//#[test]
//fn task_def_parse() {
//    // language=toml
//    let node = r#"
//        proc = "update"
//        [watch]
//        "$conf.hosts" = "+"
//        [watch_file]
//        "conf/hosts/**" = "+"
//
//"#;
//    let node: NodeRef = node!(node, toml);
//    let model: Model = Model::empty();
//
//    let proc = ProcDef::parse(&model, model.as_scoped(), &node).unwrap_disp();
//
//}
