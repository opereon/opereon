use crate::Sha1Hash;
use git2::build::CheckoutBuilder;
use git2::{
    Commit, ErrorCode, IndexAddOption, Odb, Oid, Repository, RepositoryInitOptions, Signature, Tree,
};
use kg_diag::Severity;
use kg_diag::{BasicDiag, ResultExt};
use serde::export::fmt::Debug;
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
        fmt = "cannot get git file '{file_display}': '{err}'",
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

    #[display(fmt = "cannot set config key '{key}': '{err}'")]
    SetConfig { key: String, err: git2::Error },

    #[display(fmt = "git error occurred: '{err}'")]
    Custom { err: git2::Error },
}

/// Struct to manage git repository
pub struct GitManager {
    /// Contains opened repository
    repo: Repository,
}

impl GitManager {
    /// Open existing git repository and return `GitManager`
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
    pub fn commit(&self, message: &str) -> GitResult<Sha1Hash> {
        let sig = self
            .repo()
            .signature()
            .map_err(|err| GitErrorDetail::Custom { err })?;

        self.commit_sign(message, &sig)
    }

    pub fn commit_sign(&self, message: &str, signature: &Signature) -> GitResult<Sha1Hash> {
        let repo = self.repo();

        let oid = self.update_index()?;
        let parent = self.find_last_commit()?;
        let tree = self.get_tree(&oid.into())?;

        let commit = if let Some(parent) = parent {
            repo.commit(
                Some("HEAD"),
                signature,
                signature,
                message,
                &tree,
                &[&parent],
            )
            .map_err(|err| GitErrorDetail::Commit { err })?
        } else {
            repo.commit(Some("HEAD"), signature, signature, message, &tree, &[])
                .map_err(|err| GitErrorDetail::Commit { err })?
        };

        let mut opts = CheckoutBuilder::new();
        repo.checkout_index(None, Some(&mut opts))
            .map_err(|err| GitErrorDetail::Custom { err })?;
        Ok(commit.into())
    }

    /// Creates new git repository and makes initial commit.
    pub fn init_new_repository<P: AsRef<Path>>(
        path: P,
        opts: &RepositoryInitOptions,
    ) -> GitResult<()> {
        let repo = Repository::init_opts(path.as_ref(), opts)
            .map_err(|err| GitErrorDetail::CreateRepository { err })
            .into_diag()?;
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
        let opts = IndexAddOption::default();
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

impl Debug for GitManager {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "GitManager")
    }
}
