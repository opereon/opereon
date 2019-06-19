use super::*;
use git2::{Oid, Repository, DiffOptions, DiffFindOptions, ObjectType};


#[derive(Debug)]
pub struct ModelUpdate<'a> {
    cache: NodePathCache,
    model_diff: ModelDiff,
    file_diff: FileDiff,
    matcher1_r: NodePathMatcher,
    matcher2_a: NodePathMatcher,
    matcher2_u: NodePathMatcher,
    model1: &'a Model,
    model2: &'a Model,
}

impl<'a> ModelUpdate<'a> {
    pub fn new(model1: &'a Model, model2: &'a Model) -> ModelUpdate<'a> {
        let mut cache = NodePathCache::new();
        let model_diff = ModelDiff::full_cache(model1.root(), model2.root(), &mut cache);
        let file_diff = FileDiff::minimal(model1, model2);
        ModelUpdate {
            cache,
            model_diff,
            file_diff,
            matcher1_r: NodePathMatcher::new(),
            matcher2_a: NodePathMatcher::new(),
            matcher2_u: NodePathMatcher::new(),
            model1,
            model2,
        }
    }

    pub fn model_diff(&self) -> &ModelDiff {
        &self.model_diff
    }

    pub fn check_updater(&mut self, u: &ProcDef) -> (Vec<&ModelChange>, Vec<&FileChange>) {
        debug_assert!(u.kind() == ProcKind::Update);

        self.matcher1_r.clear();
        self.matcher2_a.clear();
        self.matcher2_u.clear();

        let root1 = self.model1.root();
        let scope1 = self.model1.scope();
        let root2 = self.model2.root();
        let scope2 = self.model2.scope();

        for mw in u.model_watches().iter() {
            if mw.mask().has_removed() {
                self.matcher1_r.resolve_ext_cache(mw.path(), root1, root1, &scope1, &mut self.cache);
            }
            if mw.mask().has_added() {
                self.matcher2_a.resolve_ext_cache(mw.path(), root2, root2, &scope2, &mut self.cache);
            }
            if mw.mask().has_updated() {
                self.matcher2_u.resolve_ext_cache(mw.path(), root2, root2, &scope2, &mut self.cache);
            }
        }

        let mut model_changes = Vec::new();
        for c in self.model_diff.changes().iter() {
            match c.kind() {
                ChangeKind::Removed => if self.matcher1_r.matches(c.path()) {
                    model_changes.push(c)
                }
                ChangeKind::Added => if self.matcher2_a.matches(c.path()) {
                    model_changes.push(c)
                }
                ChangeKind::Updated => if self.matcher2_u.matches(c.path()) {
                    model_changes.push(c)
                }
                ChangeKind::Renamed => {
                    unreachable!()
                }
            }
        }

        let fw: &[FileWatch] = u.file_watches();

        let removed_watches:Vec<&FileWatch> = fw.iter().filter(|w| w.mask().has_removed()).collect();
        let added_watches:Vec<&FileWatch> = fw.iter().filter(|w| w.mask().has_added()).collect();
        let updated_watches:Vec<&FileWatch> = fw.iter().filter(|w| w.mask().has_updated()).collect();
        let renamed_watches:Vec<&FileWatch> = fw.iter().filter(|w| w.mask().has_renamed()).collect();

        let mut file_changes = Vec::new();
        for c in self.file_diff.changes().iter() {
            match c.kind() {
                ChangeKind::Removed => {
                    removed_watches.iter()
                        .filter(|w| w.glob().compile_matcher().is_match(c.old_path().unwrap()))
                        .for_each(|w| {
                            file_changes.push(c);
                        });
                }
                ChangeKind::Added => {
                    added_watches.iter()
                        .filter(|w| w.glob().compile_matcher().is_match(c.new_path().unwrap()))
                        .for_each(|w| {
                            file_changes.push(c);
                        });
                }
                ChangeKind::Updated => {
                    updated_watches.iter()
                        .filter(|w| w.glob().compile_matcher().is_match(c.new_path().unwrap()))
                        .for_each(|w| {
                            file_changes.push(c);
                        });
                }
                ChangeKind::Renamed => {
                    renamed_watches.iter()
                        .filter(|w| w.glob().compile_matcher().is_match(c.old_path().unwrap()))
                        .for_each(|w| {
                            file_changes.push(c);
                        });
                }
            }
        }
        (model_changes, file_changes)
    }
}



fn delta_to_change_kind(delta: git2::Delta) -> ChangeKind {
    use git2::*;
    match delta {
        Delta::Added => ChangeKind::Added,
        Delta::Deleted => ChangeKind::Removed,
        Delta::Modified => ChangeKind::Updated,
        Delta::Renamed => ChangeKind::Renamed,
        _ => { panic!("Unsupported") }
    }
}

/// Represents single physical model change (see https://docs.rs/git2/0.9.1/git2/struct.DiffDelta.html)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    kind: ChangeKind,
    old_path: Option<PathBuf>,
    new_path: Option<PathBuf>,
}

impl FileChange {
    pub fn new(kind: ChangeKind, old: Option<PathBuf>, new: Option<PathBuf>) -> Self {
        Self {
            kind,
            old_path: old,
            new_path: new,
        }
    }

    pub fn kind(&self) -> ChangeKind {
        self.kind
    }

    pub fn old_path(&self) -> Option<&PathBuf> {
        self.old_path.as_ref()
    }

    pub fn new_path(&self) -> Option<&PathBuf> {
        self.new_path.as_ref()
    }
}

impl From<git2::DiffDelta<'_>> for FileChange {
    fn from(diff_delta: git2::DiffDelta) -> Self {
        let kind = delta_to_change_kind(diff_delta.status());

        let old = if diff_delta.old_file().id().is_zero() {
            None
        } else {
            Some(diff_delta.old_file().path().expect("Path cannot be None!").to_owned())
        };

        let new = if diff_delta.new_file().id().is_zero() {
            None
        } else {
            Some(diff_delta.new_file().path().expect("Path cannot be None!").to_owned())
        };

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

        let old_commit = repo.find_object(old, None).expect("Cannot find commit");
        let new_commit = repo.find_object(new, None).expect("Cannot find commit");

        let old_tree = old_commit.peel_to_tree().expect("Cannot get commit tree");
        let new_tree = new_commit.peel_to_tree().expect("Cannot get commit tree");

        let mut opts = DiffOptions::new();
        opts.minimal(true);

        let mut diff = repo.diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut opts)).expect("Cannot get diff");

        let mut find_opts = DiffFindOptions::new();
        find_opts.renames(true);
        find_opts.renames_from_rewrites(true);
        find_opts.remove_unmodified(true);

        diff.find_similar(Some(&mut find_opts)).expect("Cannot find similar!");

        let changes = diff.deltas().map(|d| d.into()).collect();

        Self {
            changes
        }
    }

    pub fn changes(&self) -> &Vec<FileChange> {
        &self.changes
    }
}

