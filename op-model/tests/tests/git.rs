use super::*;
use git2::{Repository, RepositoryInitOptions, Signature};
use op_model::GitErrorDetail;
use op_model::{GitManager, Sha1Hash};
use std::path::Path;

#[test]
fn new_git_manager_empty_repo() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    GitManager::new(dir).unwrap_disp();
}

#[test]
fn new_git_manager_no_repo() {
    let (_tmp, dir) = get_tmp_dir();
    let res = GitManager::new(dir);

    let (_err, _detail) = assert_detail!(res, GitErrorDetail, GitErrorDetail::OpenRepository{..});
}

#[test]
fn init() {
    let (_tmp, dir) = get_tmp_dir();
    let opts = RepositoryInitOptions::new();
    GitManager::init_new_repository(&dir, &opts).unwrap_disp();

    assert!(dir.join(".git").exists())
}

#[test]
fn init_err() {
    let (_tmp, dir) = get_tmp_dir();
    let mut opts = RepositoryInitOptions::new();
    opts.no_reinit(true);
    init_repo(&dir);

    let res = GitManager::init_new_repository(&dir, &opts);

    let (_err, _detail) = assert_detail!(res, GitErrorDetail, GitErrorDetail::CreateRepository{..});
}

#[test]
fn commit_not_empty_repo() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    let sign = Signature::now("Test", "test@test.com").unwrap();
    write_file!(dir.join("example_file.txt"), "example content");
    initial_commit(&dir);

    let git = GitManager::new(&dir).unwrap_disp();
    let oid = git.commit_sign("test commit", &sign).unwrap_disp();

    let repo = Repository::open(&dir).unwrap();
    let head = repo.head().unwrap();
    let obj = head.resolve().unwrap();
    let commit = obj.peel_to_commit().unwrap().id();
    assert_eq!(commit, oid.as_oid());
}

#[test]
fn commit_empty_repo() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    let sign = Signature::now("Test", "test@test.com").unwrap();
    write_file!(dir.join("example_file.txt"), "example content");

    let git = GitManager::new(&dir).unwrap_disp();
    let oid = git.commit_sign("test commit", &sign).unwrap_disp();

    let repo = Repository::open(&dir).unwrap();
    let head = repo.head().unwrap();
    let obj = head.resolve().unwrap();
    let commit = obj.peel_to_commit().unwrap().id();
    assert_eq!(commit, oid.as_oid());
}

#[test]
fn update_index_empty_repo() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    write_file!(dir.join("example_file.txt"), "example content");
    write_file!(dir.join("ignored_file.txt"), "content of ignored file");
    write_file!(dir.join(".gitignore"), "ignored_file.txt");

    let git = GitManager::new(&dir).unwrap_disp();
    git.update_index().unwrap_disp();

    let repo = Repository::open(&dir).unwrap();
    let index = repo.index().unwrap();

    assert_eq!(2, index.iter().count());
    let example = index
        .iter()
        .find(|ie| String::from_utf8_lossy(&ie.path) == "example_file.txt");
    let gitignore = index
        .iter()
        .find(|ie| String::from_utf8_lossy(&ie.path) == ".gitignore");
    let ignored = index
        .iter()
        .find(|ie| String::from_utf8_lossy(&ie.path) == "ignored_file.txt");
    assert!(example.is_some());
    assert!(gitignore.is_some());
    assert!(ignored.is_none());
}

#[test]
fn resolve_revision_str_err() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    let git = GitManager::new(dir).unwrap_disp();

    let res = git.resolve_revision_str("HEAD");

    let (_err, _detail) = assert_detail!(res, GitErrorDetail, GitErrorDetail::RevisionNotFound{..});
}

#[test]
fn resolve_revision_str() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    let commit = initial_commit(&dir);
    let git = GitManager::new(dir).unwrap_disp();

    let res = git.resolve_revision_str("HEAD").unwrap_disp();

    assert_eq!(commit, res);
}

#[test]
fn read_obj_data() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    write_file!(dir.join("example_file.txt"), "example content");
    let commit = initial_commit(&dir);
    let git = GitManager::new(dir).unwrap_disp();

    let tree = git.get_tree(&commit).unwrap_disp();

    let content = git.read_obj_data(&tree, "example_file.txt").unwrap_disp();

    assert_eq!(String::from_utf8_lossy(&content), "example content");
}

#[test]
fn read_obj_data_not_found() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    write_file!(dir.join("example_file.txt"), "example content");
    let commit = initial_commit(&dir);
    let git = GitManager::new(dir).unwrap_disp();

    let tree = git.get_tree(&commit).unwrap_disp();

    let res = git.read_obj_data(&tree, "non_existing_file.txt");

    let (_err, _detail) = assert_detail!(res, GitErrorDetail, GitErrorDetail::GetFile{..});
}
