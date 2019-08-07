use super::*;
use kg_tree::Kind;
use op_model::{HostDef, ParsedModelDef};

#[test]
fn new_host_empty() {
    let node: NodeRef = node!("{}");
    let res = HostDef::new(node.clone(), node.clone()).unwrap_disp();

    assert!(res.hostname().is_empty())
}

#[test]
fn new_host_hostname() {
    let node: NodeRef = node!(r#"{"hostname": "localhost"}"#);
    let host = HostDef::new(node.clone(), node.clone()).unwrap_disp();

    assert_eq!("localhost", host.hostname())
}

#[test]
fn new_host_fqdn() {
    // language=json
    let json = r#"{"fqdn": "localhost"}"#;
    let node: NodeRef = node!(json);
    let host = HostDef::new(node.clone(), node.clone()).unwrap_disp();

    assert_eq!("localhost", host.hostname())
}

#[test]
fn parse() {
    // language=json
    let node = r#"{
        "hostname": "localhost",
        "ssh_dest": {}
    }"#;
    let node: NodeRef = node!(node);
    let model: Model = Model::empty();

    let host = HostDef::parse(&model, model.as_scoped(), &node).unwrap_disp();

    assert_eq!("localhost", host.hostname())
}

#[test]
fn parse_missing_hostname() {
    // language=json
    let node = r#"{
        "ssh_dest": {}
    }"#;
    let node: NodeRef = node!(node);
    let model: Model = Model::empty();

    let res = HostDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) =
        assert_detail!(res, DefsErrorDetail, DefsErrorDetail::HostMissingHostname);
}

#[test]
fn parse_missing_ssh_dest() {
    // language=json
    let node = r#"{
        "hostname": "localhost"
    }"#;
    let node: NodeRef = node!(node);
    let model: Model = Model::empty();

    let res = HostDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(res, DefsErrorDetail, DefsErrorDetail::HostMissingSshDest);
}

#[test]
fn parse_non_obj_host() {
    // language=json
    let node = r#""some string""#;
    let node: NodeRef = node!(node);
    let model: Model = Model::empty();

    let res = HostDef::parse(&model, model.as_scoped(), &node);

    let (_err, _detail) = assert_detail!(
        res,
        DefsErrorDetail,
        DefsErrorDetail::HostNonObject { kind },
        kind == &Kind::String
    );
}
