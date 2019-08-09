use super::*;
use globset::GlobBuilder;
use kg_tree::opath::Opath;
use op_model::{FileWatch, ModelWatch};

#[test]
fn model_watch_parse() {
    let w = ModelWatch::parse("$conf.hosts", "+").unwrap_disp();

    assert_eq!(&Opath::parse("$conf.hosts").unwrap(), w.path());
    assert!(w.mask().has_added())
}

#[test]
fn model_watch_parse_err() {
    let res = ModelWatch::parse("@.@", "+");

    let (_err, _detail) =
        assert_detail!(res, DefsErrorDetail, DefsErrorDetail::ProcModelWatchParseErr{..});
}

#[test]
fn file_watch_parse() {
    let w = FileWatch::parse("conf/hosts/**", "+").unwrap_disp();

    let glob = GlobBuilder::new("conf/hosts/**").build().unwrap();
    assert_eq!(&glob, w.glob());
    assert!(w.mask().has_added())
}

#[test]
fn file_watch_parse_err() {
    let res = FileWatch::parse("[Z-A]", "+");

    let (_err, _detail) =
        assert_detail!(res, DefsErrorDetail, DefsErrorDetail::ProcFileWatchParseErr{..});
}
