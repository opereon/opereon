use super::*;
use op_model::{AsScoped, ParsedModelDef, UserDef};
use op_test_helpers::UnwrapDisplay;

#[test]
fn new_empty() {
    // language=toml
    let node = r#""#;
    let node: NodeRef = node!(node, toml);

    let ud = UserDef::new(node.clone(), node.clone()).unwrap_disp();

    assert_eq!("", ud.username())
}

#[test]
fn new_with_username() {
    // language=toml
    let node = r#"
    username = "root"
"#;
    let node: NodeRef = node!(node, toml);

    let ud = UserDef::new(node.clone(), node.clone()).unwrap_disp();

    assert_eq!("root", ud.username())
}

#[test]
fn parse_with_username() {
    // language=toml
    let node = r#"
    username = "root"
"#;
    let node: NodeRef = node!(node, toml);
    let model: Model = Model::empty();

    let ud = UserDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!("root", ud.username())
}

#[test]
fn parse_without_username() {
    // language=toml
    let node = r#""#;
    let node: NodeRef = node!(node, toml);
    let model: Model = Model::empty();

    let res = UserDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) =
        assert_detail!(res, DefsErrorDetail, DefsErrorDetail::UserMissingUsername);
}

#[test]
fn parse_non_object() {
    // language=json
    let node = r#" "string node""#;
    let node: NodeRef = node!(node, json);
    let model: Model = Model::empty();

    let res = UserDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::UserNonObject { kind },
        assert_eq!("string", kind.as_str())
    );
}
