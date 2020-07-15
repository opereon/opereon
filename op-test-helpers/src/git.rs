use super::*;
pub fn init_repo<P: AsRef<Path>>(path: P) -> git2::Repository {
    git2::Repository::init(path).expect("Cannot init git repository!")
}

pub fn initial_commit(path: &Path) -> git2::Oid {
    let repo = git2::Repository::open(path).unwrap();
    let sig = repo.signature().unwrap();

    let tree_id = {
        let mut index = repo.index().unwrap();
        index
            .add_all(&["*"], git2::IndexAddOption::default(), None)
            .unwrap();
        index.write_tree().unwrap()
    };

    let tree = repo.find_tree(tree_id).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "Initial commit", &tree, &[])
        .unwrap()
}
