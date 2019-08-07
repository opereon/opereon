use super::*;
use kg_tree::opath::Opath;
use op_model::ValueDef;

#[test]
fn value_parse_static() {
    // language=json
    let node = r#"{
        "some key": "some value"
    }"#;
    let node: NodeRef = node!(node);

    let val = ValueDef::parse(&node).unwrap_disp();

    if let ValueDef::Static(n) = val {
        assert_eq!(n, node);
    } else {
        panic!()
    }
}

#[test]
fn value_parse_static_string() {
    // language=json
    let node = r#""some string""#;
    let node: NodeRef = node!(node);

    let val = ValueDef::parse(&node).unwrap_disp();

    if let ValueDef::Static(n) = val {
        assert_eq!(n, node);
    } else {
        panic!()
    }
}

#[test]
fn value_parse_resolvable() {
    // language=json
    let node = r#""${'some expression' + 1}""#;
    let node: NodeRef = node!(node);

    let val = ValueDef::parse(&node).unwrap_disp();

    if let ValueDef::Resolvable(opath) = val {
        let expected = Opath::parse("'some expression' + 1").unwrap_disp();
        assert_eq!(opath, expected)
    } else {
        panic!()
    }
}

#[test]
fn value_parse_resolvable_opath_err() {
    // language=json
    let node = r#""${ @.@ }""#;
    let node: NodeRef = node!(node);

    let res = ValueDef::parse(&node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::OpathParseErr{..});

    eprintln!("_err = {}", _err);

    // TODO check error message
}
