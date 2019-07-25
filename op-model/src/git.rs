use crate::Sha1Hash;
use git2::{
    Commit, ErrorCode, IndexAddOption, Odb, Oid, Repository, RepositoryInitOptions, Signature, Tree,
};
use kg_diag::Severity;
use kg_diag::{BasicDiag, ResultExt};
use std::path::{Path, PathBuf};

pub type GitError = BasicDiag;
pub type GitResult<T> = Result<T, GitError>;

#[derive(Debug, Display, Detail)]
#[diag(code_offset = 1100)]
pub enum GitErrorDetail {
    #[display(fmt = "cannot open git repository: '{err}'")]
    OpenRepository { err: git2::Error },

    #[display(fmt = "cannot create git repository: '{err}'")]
    CreateRepository { err: git2::Error },

    #[display(fmt = "cannot get git object database: '{err}'")]
    GetDatabase { err: git2::Error },

    #[display(fmt = "cannot create commit: '{err}'")]
    Commit { err: git2::Error },

    #[display(fmt = "cannot get git index: '{err}'")]
    GetIndex { err: git2::Error },

    #[display(
        fmt = "cannot get file '{file_display}': '{err}'",
        file_display = "file.display()"
    )]
    GetFile { file: PathBuf, err: git2::Error },

    #[display(fmt = "cannot find git object: '{err}'")]
    FindObject { err: git2::Error },

    #[display(fmt = "cannot resolve git reference: '{err}'")]
    ResolveReference { err: git2::Error },

    #[display(fmt = "cannot find revision: '{err}'")]
    RevisionNotFound { err: git2::Error },

    #[display(fmt = "unexpected git object type: '{err}'")]
    UnexpectedObjectType { err: git2::Error },

    #[display(fmt = "git error occurred: '{err}'")]
    Custom { err: git2::Error },
}

/// Struct to manage git repository
pub struct GitManager {
    /// Contains opened repository or `None`
    repo: Repository,
}

impl GitManager {
    pub fn new<P: AsRef<Path>>(repo_dir: P) -> GitResult<Self> {
        let repo: Repository = Repository::open(repo_dir.as_ref())
            .map_err(|err| BasicDiag::from(GitErrorDetail::OpenRepository { err }))?;
        Ok(Self { repo })
    }

    fn repo(&self) -> &Repository {
        &self.repo
    }

    /// Refresh index, commit current changes and checkout to this commit.
    /// Returns oid of created commit
    pub fn commit(&self, message: &str, signature: &Signature) -> GitResult<Sha1Hash> {
        let repo = self.repo();

        let oid = self.update_index()?;
        let parent = self.find_last_commit()?;
        let tree = self.get_tree(&oid.into())?;

        if let Some(parent) = parent {
            let _commit = repo
                .commit(
                    Some("HEAD"),
                    signature,
                    signature,
                    message,
                    &tree,
                    &[&parent],
                )
                .map_err(|err| GitErrorDetail::Commit { err })?;
        } else {
            let _commit = repo
                .commit(Some("HEAD"), signature, signature, message, &tree, &[])
                .map_err(|err| GitErrorDetail::Commit { err })?;
        };

        repo.checkout_index(None, None)
            .map_err(|err| GitErrorDetail::Custom { err })?;
        Ok(oid.into())
    }

    /// Creates new git repository
    pub fn init_new_repository<P: AsRef<Path>>(
        path: P,
        opts: &RepositoryInitOptions,
    ) -> GitResult<()> {
        let _repo = Repository::init_opts(path.as_ref(), opts)
            .map_err(|err| GitErrorDetail::CreateRepository { err })
            .into_diag()?;
        Ok(())
    }

    /// Resolves revision string to git object id (Sha1Hash)
    pub fn resolve_revision_str(&self, spec: &str) -> GitResult<Sha1Hash> {
        let obj = self
            .repo()
            .revparse_single(spec)
            .map_err(|err| GitErrorDetail::RevisionNotFound { err })?;
        Ok(obj.id().into())
    }

    /// Returns last commit or `None` if repository have no commits (eg. new repository).
    fn find_last_commit(&self) -> GitResult<Option<Commit>> {
        let obj = match self.repo().head() {
            Ok(head) => head,
            Err(err) => match err.code() {
                ErrorCode::UnbornBranch => return Ok(None),
                _ => {
                    return Err(GitErrorDetail::FindObject { err }).into_diag();
                }
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
    pub fn get_tree(&self, oid: &Sha1Hash) -> GitResult<Tree> {
        let obj = self
            .repo()
            .find_object(oid.as_oid(), None)
            .map_err(|err| GitErrorDetail::FindObject { err })?;

        let tree = obj
            .peel_to_tree()
            .map_err(|err| GitErrorDetail::UnexpectedObjectType { err })?;
        Ok(tree)
    }

    /// Update provided repository index and return created tree Oid.
    /// Clear index and rebuild it from working dir. Necessary to reflect .gitignore changes.
    pub fn update_index(&self) -> GitResult<Oid> {
        let mut index = self
            .repo()
            .index()
            .map_err(|err| GitErrorDetail::GetIndex { err })?;

        index
            .clear()
            .map_err(|err| GitErrorDetail::Custom { err })?;
        index
            .add_all(&["*"], IndexAddOption::default(), None)
            .map_err(|err| GitErrorDetail::Custom { err })?;

        // Changes in index won't be saved to disk until index.write*() called.
        let oid = index
            .write_tree()
            .map_err(|err| GitErrorDetail::Custom { err })?;
        Ok(oid)
    }

    /// Searches `tree` for object under `path` and returns its data.
    pub fn read_obj_data<P: AsRef<Path>>(&self, tree: &Tree, path: P) -> GitResult<Vec<u8>> {
        let odb = self
            .repo()
            .odb()
            .map_err(|err| GitErrorDetail::Custom { err })?;

        let entry = tree
            .get_path(path.as_ref())
            .map_err(|err| GitErrorDetail::GetFile {
                file: path.as_ref().to_path_buf(),
                err,
            })?;
        let obj = odb
            .read(entry.id())
            .map_err(|err| GitErrorDetail::GetFile {
                file: path.as_ref().to_path_buf(),
                err,
            })?;

        Ok(obj.data().to_owned())
    }

    pub fn odb(&self) -> GitResult<Odb> {
        self.repo()
            .odb()
            .map_err(|err| GitErrorDetail::Custom { err })
            .into_diag()
    }
}

/*#[cfg(test)]
mod tests {
    use git2::build::CheckoutBuilder;
    use git2::{DiffFindOptions, DiffFormat, DiffOptions, Index, IndexAddOption, ObjectType, Oid};

    use super::*;

    #[test]
    fn checkout_to_dir() {
        let current = PathBuf::from("/home/wiktor/Desktop/opereon/resources/model");
        let out_dir = current.join(".op/checked_out");

        let commit_hash = Oid::from_str("996d94321d833a918842c69531197f9d368ec4b6")
            .expect("Cannot parse commit hash");

        let repo = Repository::open(&current).expect("Cannot open repository");

        let commit = repo.find_commit(commit_hash).expect("Cannot find commit");
        let tree = commit.tree().expect("Cannot get commit tree");

        let mut builder = CheckoutBuilder::new();
        builder.target_dir(&out_dir);
        // cannot update current index
        builder.update_index(false);
        // override everything in out_dir with commit state
        builder.force();

        repo.checkout_tree(tree.as_object(), Some(&mut builder))
            .expect("Cannot checkout tree!");
    }

    #[test]
    fn diff() {
        let current = PathBuf::from("/home/wiktor/Desktop/opereon/resources/model");

        let commit_hash1 = Oid::from_str("6f09d0ad3908daa16992656cb33d4ed075e554a8")
            .expect("Cannot parse commit hash");

        let repo = Repository::open(&current).expect("Cannot open repository");

        let commit1 = repo.find_commit(commit_hash1).expect("Cannot find commit");
        let tree1 = commit1.tree().expect("Cannot get commit tree");

        let mut opts = DiffOptions::new();
        opts.minimal(true);

        let mut index = repo.index().expect("Cannot get index!");

        //         TODO what about .operc [[exclude]]? Should it be equal to .gitignore?
        // Clear index and rebuild it from working dir. Necessary to reflect .gitignore changes
        // Changes in index won't be saved to disk until index.write*() called.
        index.clear().expect("Cannot clear index");
        index
            .add_all(&["*"], IndexAddOption::default(), None)
            .expect("Cannot update index");

        //        index.write().expect("cannot write index");

        let mut diff = repo
            .diff_tree_to_workdir_with_index(Some(&tree1), Some(&mut opts))
            .expect("Cannot get diff");

        let mut find_opts = DiffFindOptions::new();
        find_opts.renames(true);
        find_opts.renames_from_rewrites(true);
        find_opts.remove_unmodified(true);

        diff.find_similar(Some(&mut find_opts))
            .expect("Cannot find similar!");
        println!("Diffs:");

        let deltas = diff.deltas();
        eprintln!("deltas.size_hint() = {:?}", deltas.size_hint());
        for delta in deltas {
            println!("======");
            eprintln!("Change type: {:?}", delta.status());
            let old = delta.old_file();
            let new = delta.new_file();
            eprintln!("old = id: {:?}, path: {:?}", old.id(), old.path());
            eprintln!("new = id: {:?}, path: {:?}", new.id(), new.path());
        }
    }
}*/
