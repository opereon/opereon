use super::*;
use git2::{Repository, RepositoryInitOptions, Signature};
use op_rev::{GitErrorDetail, FileVersionManager, RevPath};
use op_rev::GitManager;
use op_test_helpers::{get_tmp_dir, init_repo, ToStringExt, initial_commit, UnwrapDisplay};

#[test]
fn new_git_manager_empty_repo() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    aw!(GitManager::open(dir)).unwrap_disp();
}

#[test]
fn new_git_manager_no_repo() {
    let (_tmp, dir) = get_tmp_dir();
    let res = aw!(GitManager::open(dir));

    let (_err, _detail) = assert_detail!(res, GitErrorDetail, GitErrorDetail::OpenRepository{..});
}

#[test]
fn init() {
    let (_tmp, dir) = get_tmp_dir();
    aw!(GitManager::create(&dir)).unwrap_disp();

    assert!(dir.join(".git").exists())
}

#[test]
fn init_err() {
    let (_tmp, dir) = get_tmp_dir();
    let mut opts = RepositoryInitOptions::new();
    opts.no_reinit(true);
    init_repo(&dir);

    let res = aw!(GitManager::create(&dir));

    let (_err, _detail) = assert_detail!(res, GitErrorDetail, GitErrorDetail::CreateRepository{..});
}

// #[test]
// fn commit_not_empty_repo() {
//     let (_tmp, dir) = get_tmp_dir();
//     init_repo(&dir);
//     let sign = Signature::now("Test", "test@test.com").unwrap();
//     write_file!(dir.join("example_file.txt"), "example content");
//     initial_commit(&dir);
//
//     let git = aw!(GitManager::open(dir)).unwrap_disp();
//     let oid = git.commit_sign("test commit", &sign).unwrap_disp();
//
//     let repo = Repository::open(&dir).unwrap();
//     let head = repo.head().unwrap();
//     let obj = head.resolve().unwrap();
//     let commit = obj.peel_to_commit().unwrap().id();
//     assert_eq!(commit, oid.as_oid());
// }

// #[test]
// fn commit_empty_repo() {
//     let (_tmp, dir) = get_tmp_dir();
//     init_repo(&dir);
//     let sign = Signature::now("Test", "test@test.com").unwrap();
//     write_file!(dir.join("example_file.txt"), "example content");
//
//     let git = aw!(GitManager::open(dir)).unwrap_disp();
//     let oid = git.commit_sign("test commit", &sign).unwrap_disp();
//
//     let repo = Repository::open(&dir).unwrap();
//     let head = repo.head().unwrap();
//     let obj = head.resolve().unwrap();
//     let commit = obj.peel_to_commit().unwrap().id();
//     assert_eq!(commit, oid.as_oid());
// }

#[test]
fn update_index_empty_repo() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    write_file!(dir.join("example_file.txt"), "example content");
    write_file!(dir.join("ignored_file.txt"), "content of ignored file");
    write_file!(dir.join(".gitignore"), "ignored_file.txt");

    let git = aw!(GitManager::open(dir.clone())).unwrap_disp();
    git.update_index().unwrap_disp();

    let repo = Repository::open(&dir).unwrap();
    let index = repo.index().unwrap();

    assert_eq!(2, index.iter().count());
    let example = index
        .iter()
        .find(|ie| ie.path.to_string_ext() == "example_file.txt");
    let gitignore = index
        .iter()
        .find(|ie| ie.path.to_string_ext() == ".gitignore");
    let ignored = index
        .iter()
        .find(|ie| ie.path.to_string_ext() == "ignored_file.txt");
    assert!(example.is_some());
    assert!(gitignore.is_some());
    assert!(ignored.is_none());
}

#[test]
fn resolve_revision_str_err() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    let mut git = aw!(GitManager::open(dir)).unwrap_disp();

    let res = aw!(git.resolve(&RevPath::Revision("HEAD".to_string())));

    let (_err, _detail) = assert_detail!(res, GitErrorDetail, GitErrorDetail::RevisionNotFound{..});
}

#[test]
fn resolve_revision_str() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    let commit = initial_commit(&dir);
    let mut git = aw!(GitManager::open(dir)).unwrap_disp();

    let res = aw!(git.resolve(&RevPath::Revision("HEAD".to_string()))).unwrap_disp();

    assert_eq!(commit, res.into());
}
