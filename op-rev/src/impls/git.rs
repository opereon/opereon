use super::*;

use git2::build::CheckoutBuilder;

use kg_diag::io::fs;
use kg_diag::{BasicDiag, IntoDiagRes};

pub type GitResult<T> = Result<T, BasicDiag>;

#[derive(Debug, Display, Detail)]
#[diag(code_offset = 1100)]
pub enum GitErrorDetail {
    #[display(fmt = "cannot open git repository: {err}")]
    OpenRepository { err: git2::Error },

    #[display(fmt = "cannot create git repository: {err}")]
    CreateRepository { err: git2::Error },

    #[display(fmt = "cannot create commit: {err}")]
    Commit { err: git2::Error },

    #[display(fmt = "cannot checkout tree {rev_id}: {err}")]
    Checkout { rev_id: Oid, err: git2::Error },

    #[display(fmt = "cannot get git index: {err}")]
    GetIndex { err: git2::Error },

    #[display(fmt = "cannot resolve git reference: {err}")]
    ResolveReference { err: git2::Error },

    #[display(fmt = "cannot find revision: {err}")]
    RevisionNotFound { err: git2::Error },

    #[display(fmt = "unexpected git object type: {err}")]
    UnexpectedObjectType { err: git2::Error },

    #[display(fmt = "cannot set config key '{key}': {err}")]
    SetConfig { key: String, err: git2::Error },

    #[display(fmt = "git error occurred: {err}")]
    Custom { err: git2::Error },
}


/// Struct to manage git repository
pub struct GitManager {
    path: PathBuf,
    /// Contains opened repository
    repo: git2::Repository,
}

impl GitManager {
    /// Open existing git repository and return `GitManager`
    pub fn open<P: Into<PathBuf>>(repo_dir: P) -> GitResult<Self> {
        let path = repo_dir.into();
        let repo = git2::Repository::open(&path)
            .map_err(|err| BasicDiag::from(GitErrorDetail::OpenRepository { err }))?;
        Ok(GitManager {
            path,
            repo,
        })
    }

    /// Create a new git repository and return `GitManager`
    pub fn create<P: Into<PathBuf>>(repo_dir: P) -> GitResult<Self> {
        use std::fmt::Write;

        let path = repo_dir.into();

        let mut opts = git2::RepositoryInitOptions::new();
        opts.no_reinit(true);

        let repo = git2::Repository::init_opts(&path, &opts)
            .map_err(|err| GitErrorDetail::CreateRepository { err })?;

        let mut config = repo.config().unwrap();

        // TODO parametrize
        config
            .set_str("user.name", "opereon")
            .map_err(|err| GitErrorDetail::SetConfig {
                key: "user.name".to_string(),
                err,
            })?;
        config
            .set_str("user.email", "opereon")
            .map_err(|err| GitErrorDetail::SetConfig {
                key: "user.email".to_string(),
                err,
            })?;

        // ignore ./op directory
        let excludes = path.join(Path::new(".git/info/exclude"));
        let mut content = fs::read_string(&excludes)?;

        writeln!(&mut content, ".op/").map_err(IoErrorDetail::from)?;
        fs::write(excludes, content)?;

        Ok(GitManager {
            path,
            repo,
        })
    }

    fn repo(&self) -> &git2::Repository {
        &self.repo
    }

    /// Returns last commit or `None` if repository have no commits (eg. new repository).
    fn find_last_commit(&self) -> GitResult<Option<git2::Commit>> {
        let obj = match self.repo().head() {
            Ok(head) => head,
            Err(err) => match err.code() {
                git2::ErrorCode::UnbornBranch => return Ok(None),
                _ => return Err(GitErrorDetail::Custom { err }).into_diag_res(),
            },
        };

        let obj = obj
            .resolve()
            .map_err(|err| GitErrorDetail::ResolveReference { err })?;
        let commit = obj
            .peel_to_commit()
            .map_err(|err| GitErrorDetail::UnexpectedObjectType { err })?;

        Ok(Some(commit))
    }

    /// Get git tree for provided `oid`
    fn get_tree(&self, oid: Oid) -> GitResult<git2::Tree> {
        let obj = self
            .repo()
            .find_object(oid.into(), None)
            .map_err(|err| GitErrorDetail::Custom { err })?;

        let tree = obj
            .peel_to_tree()
            .map_err(|err| GitErrorDetail::UnexpectedObjectType { err })?;
        Ok(tree)
    }

    /// Update provided repository index and return created tree Oid.
    /// Clear index and rebuild it from working dir. Necessary to reflect .gitignore changes.
    fn update_index(&self) -> GitResult<git2::Oid> {
        let mut index = self
            .repo()
            .index()
            .map_err(|err| GitErrorDetail::GetIndex { err })?;

        index
            .clear()
            .map_err(|err| GitErrorDetail::Custom { err })?;

        let opts = git2::IndexAddOption::default();

        index
            .add_all(&["*"], opts, None)
            .map_err(|err| GitErrorDetail::Custom { err })?;
        // Changes in index won't be saved to disk until index.write*() called.
        let oid = index
            .write_tree()
            .map_err(|err| GitErrorDetail::Custom { err })?;
        index
            .write()
            .map_err(|err| GitErrorDetail::Custom { err })?;

        Ok(oid)
    }
}

impl std::fmt::Debug for GitManager {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_struct("GitManager")
            .field("path", &self.path)
            .finish()
    }
}

impl FileVersionManager for GitManager {
    fn resolve(&mut self, rev_path: &RevPath) -> Result<Oid, BasicDiag> {
        match *rev_path {
            RevPath::Current => Ok(Oid::nil()),
            RevPath::Revision(ref spec) => {
                let obj = self
                    .repo()
                    .revparse_single(spec)
                    .map_err(|err| GitErrorDetail::RevisionNotFound { err })?;
                Ok(obj.id().into())
            }
        }
    }

    fn checkout(&mut self, rev_id: Oid) -> Result<RevInfo, BasicDiag> {
        if rev_id.is_nil() {
            Ok(RevInfo::new(rev_id, self.path.clone()))
        } else {
            let mut checkout_path = self.path.join(".op/revs");
            checkout_path.push(&format!("{:12}", rev_id));

            if checkout_path.is_dir() {
                Ok(RevInfo::new(rev_id, checkout_path))
            } else {
                fs::create_dir_all(&checkout_path)?;

                let tree = self.get_tree(rev_id.into())?;

                let mut opts = CheckoutBuilder::new();
                opts.target_dir(&checkout_path);
                opts.recreate_missing(true);
                self.repo.checkout_tree(tree.as_object(), Some(&mut opts))
                    .map_err(|err| GitErrorDetail::Checkout { rev_id, err })?;

                Ok(RevInfo::new(rev_id, checkout_path))
            }
        }
    }

    fn commit(&mut self, message: &str) -> Result<Oid, BasicDiag> {
        let repo = self.repo();

        let sig = repo
            .signature()
            .map_err(|err| GitErrorDetail::Custom { err })?;

        let oid = self.update_index()?;
        let parent = self.find_last_commit()?;
        let tree = self.get_tree(oid.into())?;

        let commit = if let Some(parent) = parent {
            repo.commit(
                Some("HEAD"),
                &sig,
                &sig,
                message,
                &tree,
                &[&parent],
            )
                .map_err(|err| GitErrorDetail::Commit { err })?
        } else {
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
                .map_err(|err| GitErrorDetail::Commit { err })?
        };

        let mut opts = CheckoutBuilder::new();
        repo.checkout_index(None, Some(&mut opts))
            .map_err(|err| GitErrorDetail::Custom { err })?;

        Ok(commit.into())
    }

    fn get_file_diff(&mut self, old_rev_id: Oid, new_rev_id: Oid) -> Result<FileDiff, BasicDiag> {
        //FIXME (jc) error handling

        if old_rev_id.is_nil() {
            unimplemented!(); // Cannot compare workdir as old tree against new tree, only the other way around
        }

        let mut diff = {
            let mut opts = git2::DiffOptions::new();
            opts.minimal(true);

            if new_rev_id.is_nil() {
                let old: git2::Oid = old_rev_id.into();
                let old_commit = self.repo().find_object(old, None).expect("Cannot find commit");
                let old_tree = old_commit.peel_to_tree().expect("Cannot get commit tree");

                self.repo()
                    .diff_tree_to_workdir(Some(&old_tree), Some(&mut opts))
                    .expect("Cannot get diff")
            } else {
                let old: git2::Oid = old_rev_id.into();
                let old_commit = self.repo().find_object(old, None).expect("Cannot find commit");
                let old_tree = old_commit.peel_to_tree().expect("Cannot get commit tree");

                let new: git2::Oid = new_rev_id.into();
                let new_commit = self.repo().find_object(new, None).expect("Cannot find commit");
                let new_tree = new_commit.peel_to_tree().expect("Cannot get commit tree");

                self.repo()
                    .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), Some(&mut opts))
                    .expect("Cannot get diff")
            }
        };

        let mut find_opts = git2::DiffFindOptions::new();
        find_opts.renames(true);
        find_opts.renames_from_rewrites(true);
        find_opts.remove_unmodified(true);

        diff.find_similar(Some(&mut find_opts))
            .expect("Cannot find similar!");

        let changes = diff.deltas().map(|d| d.into()).collect();

        Ok(FileDiff::new(changes))
    }
}


impl From<git2::Oid> for Oid {
    fn from(oid: git2::Oid) -> Self {
        let mut hash = Oid::nil();
        hash.copy_from_slice(oid.as_bytes());
        hash
    }
}

impl Into<git2::Oid> for Oid {
    fn into(self) -> git2::Oid {
        git2::Oid::from_bytes(&self).unwrap()
    }
}


impl From<git2::DiffDelta<'_>> for FileChange {
    fn from(diff_delta: git2::DiffDelta) -> Self {
        use git2::Delta;

        let kind = match diff_delta.status() {
            Delta::Added => ChangeKind::Added,
            Delta::Deleted => ChangeKind::Removed,
            Delta::Modified => ChangeKind::Updated,
            Delta::Renamed => ChangeKind::Moved,
            _ => panic!("Unsupported"),
        };

        let old = if diff_delta.old_file().id().is_zero() {
            None
        } else {
            Some(
                diff_delta
                    .old_file()
                    .path()
                    .expect("Path cannot be None!")
                    .to_owned(),
            )
        };

        let new = if diff_delta.new_file().id().is_zero() {
            None
        } else {
            Some(
                diff_delta
                    .new_file()
                    .path()
                    .expect("Path cannot be None!")
                    .to_owned(),
            )
        };

        FileChange::new(kind, old, new)
    }
}
