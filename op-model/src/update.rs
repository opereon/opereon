use super::*;
use git2::{Oid, Repository, DiffOptions, DiffFindOptions};


#[derive(Debug)]
pub struct ModelUpdate<'a> {
    cache: NodePathCache,
    diff: ModelDiff,
    matcher1_r: NodePathMatcher,
    matcher2_a: NodePathMatcher,
    matcher2_u: NodePathMatcher,
    model1: &'a Model,
    model2: &'a Model,
}

impl<'a> ModelUpdate<'a> {
    pub fn new(model1: &'a Model, model2: &'a Model) -> ModelUpdate<'a> {
        let mut cache = NodePathCache::new();
        let diff = ModelDiff::full_cache(model1.root(), model2.root(), &mut cache);

        ModelUpdate {
            cache,
            diff,
            matcher1_r: NodePathMatcher::new(),
            matcher2_a: NodePathMatcher::new(),
            matcher2_u: NodePathMatcher::new(),
            model1,
            model2,
        }
    }

    pub fn diff(&self) -> &ModelDiff {
        &self.diff
    }

    pub fn check_updater(&mut self, u: &ProcDef) -> Vec<&ModelChange> {
        debug_assert!(u.kind() == ProcKind::Update);

        self.matcher1_r.clear();
        self.matcher2_a.clear();
        self.matcher2_u.clear();

        let root1 = self.model1.root();
        let scope1 = self.model1.scope();
        let root2 = self.model2.root();
        let scope2 = self.model2.scope();

        for w in u.watches().iter() {
            if w.mask().has_removed() {
                self.matcher1_r.resolve_ext_cache(w.path(), root1, root1, &scope1, &mut self.cache);
            }
            if w.mask().has_added() {
                self.matcher2_a.resolve_ext_cache(w.path(), root2, root2, &scope2, &mut self.cache);
            }
            if w.mask().has_updated() {
                self.matcher2_u.resolve_ext_cache(w.path(), root2, root2, &scope2, &mut self.cache);
            }
        }

        let mut changes = Vec::new();
        for c in self.diff.changes().iter() {
            match c.kind() {
                ChangeKind::Removed => if self.matcher1_r.matches(c.path()) {
                    changes.push(c)
                }
                ChangeKind::Added => if self.matcher2_a.matches(c.path()) {
                    changes.push(c)
                }
                ChangeKind::Updated => if self.matcher2_u.matches(c.path()) {
                    changes.push(c)
                }
            }
        }

        changes
    }
}


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum FileChangeKind {
    Added = 1,
    Removed = 2,
    Renamed = 3,
    Updated = 4,
}

impl From<git2::Delta> for FileChangeKind {
    fn from(delta: git2::Delta) -> Self {
        use git2::*;
        match delta {
            Delta::Added => FileChangeKind::Added,
            Delta::Deleted => FileChangeKind::Removed,
            Delta::Modified => FileChangeKind::Updated,
            Delta::Renamed => FileChangeKind::Renamed,
            _=> {panic!("Unsupported")}
        }
    }
}

/// Represents single physical model change (see https://docs.rs/git2/0.9.1/git2/struct.DiffDelta.html)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    kind: FileChangeKind,
    old: Option<PathBuf>,
    new: Option<PathBuf>
}

impl FileChange {
    pub fn new(kind: FileChangeKind, old: Option<PathBuf>, new: Option<PathBuf>) -> Self {
        Self {
            kind,
            old,
            new,
        }
    }
}

impl From<git2::DiffDelta<'_>> for FileChange {
    fn from(diff_delta: git2::DiffDelta) -> Self {
        let kind: FileChangeKind = diff_delta.status().into();
        let old = diff_delta.old_file().path().map(|p|p.to_owned());
        let new = diff_delta.old_file().path().map(|p|p.to_owned());

        FileChange::new(kind, old, new)
    }
}

/// Struct representing physical (filesystem) model changes. Operates on files stored in git.
/// This structure utilizes `git diff` functionality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    changes: Vec<FileChange>,
}


impl FileDiff {
    pub fn minimal(model1: &Model, model2: &Model) -> Self {
        // TODO ws error handling
        let repo = Repository::open(&model1.metadata().path()).expect("Cannot open repository");

        let old: Oid = model1.metadata().id().as_oid();
        let new: Oid = model2.metadata().id().as_oid();

        let old_commit = repo.find_commit(old).expect("Cannot find commit");
        let new_commit = repo.find_commit(new).expect("Cannot find commit");

        let old_tree = old_commit.tree().expect("Cannot get commit tree");
        let new_tree = new_commit.tree().expect("Cannot get commit tree");

        let mut opts = DiffOptions::new();
        opts.minimal(true);

        let mut diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut opts)).expect("Cannot get diff");

        let mut find_opts = DiffFindOptions::new();
        find_opts.renames(true);
        find_opts.renames_from_rewrites(true);
        find_opts.remove_unmodified(true);

        diff.find_similar(Some(&mut find_opts)).expect("Cannot find similar!");

        let changes = diff.deltas().map(|d|d.into()).collect();

        Self {
            changes
        }
    }
}

