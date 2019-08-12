use super::*;
use op_model::GitManager;
use std::path::{Path, PathBuf};
use git2::{Repository, Signature, RepositoryInitOptions};
use op_model::GitErrorDetail;
use std::process::exit;

fn init_repo<P: AsRef<Path>>(path: P) -> Repository {
    Repository::init(path).expect("Cannot init git repository!")
}

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
    let repo = init_repo(&dir);
    let sign = Signature::now("Test", "test@test.com").unwrap();
    write_file!(dir.join("example_file.txt"), "example content");

    let sig = repo.signature().unwrap();

    let tree_id = {
        let mut index = repo.index().unwrap();
        index.add_path(&PathBuf::from("example_file.txt")).unwrap();
        index.write_tree().unwrap()
    };

    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[]).unwrap();
    drop(tree);
    drop(repo);

    let git = GitManager::new(&dir).unwrap_disp();
    let oid = git.commit("test commit", &sign).unwrap_disp();

    let repo = Repository::open(&dir).unwrap();
    let head = repo.head().unwrap();
    let obj = head.resolve().unwrap();
    let commit = obj.peel_to_commit().unwrap().id();
    assert_eq!(commit, oid.as_oid());
}

// FIXME
//#[test]
fn commit_empty_repo() {
    let (_tmp, dir) = get_tmp_dir();
    init_repo(&dir);
    let sign = Signature::now("Test", "test@test.com").unwrap();
    write_file!(dir.join("example_file.txt"), "example content");

    let git = GitManager::new(&dir).unwrap_disp();
    let oid = git.commit("test commit", &sign).unwrap_disp();

    let repo = Repository::open(&dir).unwrap();
    let head = repo.head().unwrap();
    let obj = head.resolve().unwrap();
    let commit = obj.peel_to_commit().unwrap().id();
    assert_eq!(commit, oid.as_oid());
}

