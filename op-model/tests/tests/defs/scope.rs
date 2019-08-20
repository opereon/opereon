use super::*;
use kg_tree::opath::{Opath, ScopeMut};
use op_model::{AsScoped, ParsedModelDef, ScopeDef, ValueDef};

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

    let (err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::OpathParse{..});
    assert_cause!(err);
}

#[test]
fn scope_parse() {
    // language=toml
    let node = r#"
        scope.object.val = ["aa", "bb"]
        scope.value1 = "${@^.@key}"
        scope.value2 = 2
"#;
    let node: NodeRef = node!(node, toml);
    let model: Model = Model::empty();

    let scope = ScopeDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert!(scope.get_var_def("object").unwrap().is_static());
    assert!(!scope.get_var_def("value1").unwrap().is_static());
    assert!(scope.get_var_def("value2").unwrap().is_static());
}

#[test]
fn scope_parse_null() {
    // language=json
    let node = r#"{}"#;
    let node: NodeRef = node!(node);
    let model: Model = Model::empty();

    let scope = ScopeDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!(0, scope.len())
}

#[test]
fn scope_parse_non_object() {
    // language=toml
    let node = r#"
        scope = "invalid scope type"
"#;
    let node: NodeRef = node!(node, toml);
    let model: Model = Model::empty();

    let res = ScopeDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::ScopeNonObject { kind },
        assert_eq!("string", kind.as_str())
    );
}

#[test]
fn scope_opath_parse_err() {
    // language=toml
    let node = r#"
        scope.dyn_variable = "${@.@}"
"#;
    let node: NodeRef = node!(node, toml);
    let model: Model = Model::empty();

    let res = ScopeDef::parse(&model, model.as_scoped(), &node);

    let (err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::ScopeValParse{key, ..}, assert_eq!("dyn_variable", key));
    assert_cause!(err);
}

#[test]
fn scope_resolve() {
    // language=toml
    let node = r#"
        scope.dyn_variable = "${@.@path}"
"#;
    let node: NodeRef = node!(node, toml);
    let model: Model = Model::empty();

    let def: ScopeDef = ScopeDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    let scope = ScopeMut::new();
    def.resolve(&node, &node, &scope).unwrap_disp();

    let val = scope.get_var("dyn_variable").unwrap();
    let val = assert_one!(val.clone());

    assert_eq!("$", val.as_string_ext());
}

#[test]
fn scope_resolve_parse_err() {
    // language=toml
    let node = r#"
        scope.dyn_variable = "${array(notExistingFunc())}"
"#;
    let node: NodeRef = node!(node, toml);
    let model: Model = Model::empty();

    let def: ScopeDef = ScopeDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    let scope = ScopeMut::new();
    let res = def.resolve(&node, &node, &scope);

    let (err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::ScopeValParse{key, ..}, assert_eq!("dyn_variable", key));
    assert_cause!(err);
}
