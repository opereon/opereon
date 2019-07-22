use crate::Sha1Hash;
use git2::{Commit, ErrorCode, Index, IndexAddOption, ObjectType, Oid, Repository, RepositoryInitOptions, Signature, Tree};
use kg_diag::{BasicDiag, ResultExt};
use kg_diag::Severity;
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

    #[display(fmt = "cannot find git object: '{err}'")]
    FindObject { err: git2::Error },

    #[display(fmt = "cannot resolve git reference: '{err}'")]
    ResolveReference { err: git2::Error },

    #[display(fmt = "cannot find revision: '{err}'")]
    RevisionNotFound { err: git2::Error },

    #[display(fmt = "unexpected git object type: '{err}'")]
    UnexpectedObjectType { err: git2::Error },

    #[display(fmt = "git error occured: '{err}'")]
    Custom {
        err: git2::Error
    }
}

pub struct GitManager {

    /// Repository base dir
    repo_dir: PathBuf,

    /// Contains opened repository or `None`
    repo: Option<Repository>,
}

impl GitManager {

    pub fn new<P: AsRef<Path>>(repo_dir: P) -> Self {
        Self {
            repo_dir: repo_dir.as_ref().to_path_buf(),
            repo: None,
        }
    }

    /// Return opened git repository or open if needed
    fn repo(&mut self) -> GitResult<&mut Repository> {
        if let Some(ref mut repo) = self.repo {
            return Ok(repo);
        }

        let repo: Repository = Repository::open(&self.repo_dir)
            .map_err(|err| BasicDiag::from(GitErrorDetail::OpenRepository { err }))?;
        self.repo = Some(repo);
        Ok(self.repo.as_mut().unwrap())
    }

    fn index(&mut self) -> GitResult<Index> {
        let mut index = self.repo()?.index()
            .map_err(|err| GitErrorDetail::GetIndex{err})?;
        Ok(index)
    }

    /// Refresh index, commit current changes and checkout to this commit
    pub fn commit(&mut self, message: &str, signature: Signature) -> GitResult<()> {
        let repo = self.repo()?;

        let oid = self.update_index()?;
        let parent = self.find_last_commit()?;
        let tree = self.get_tree(&oid.into())?;

        if let Some(parent) = parent {
            let _commit = repo
                .commit(
                    Some("HEAD"),
                    &signature,
                    &signature,
                    message,
                    &tree,
                    &[&parent],
                )
                .map_err(|err| GitErrorDetail::Commit {err})?;
        } else {
            let _commit = repo
                .commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
                .map_err(|err| GitErrorDetail::Commit {err})?;
        };

        repo.checkout_index(None, None)
            .map_err(|err| GitErrorDetail::Custom {err})
            .into_diag()
    }

    /// Update provided repository index and return created tree Oid
    fn update_index(&mut self) -> GitResult<Oid> {

        // Clear index and rebuild it from working dir. Necessary to reflect .gitignore changes
        // Changes in index won't be saved to disk until index.write*() called.
        let mut index = self.index()?;

        index.clear().map_err(|err| GitErrorDetail::Custom {err})?;
        index
            .add_all(&["*"], IndexAddOption::default(), None)
            .map_err(|err| GitErrorDetail::Custom {err})?;

        // get oid of index tree
        let oid = index.write_tree().map_err(|err|GitErrorDetail::Custom {err})?;
        Ok(oid)
    }

    /// Get git tree for provided `id`
    pub fn get_tree(&mut self, id: &Sha1Hash) -> GitResult<Tree> {
        let repo = self.repo()?;
        let obj = repo
            .find_object(id.as_oid(), None)
            .map_err(|err| GitErrorDetail::FindObject { err })?;

        let tree = obj
            .peel_to_tree()
            .map_err(|err| GitErrorDetail::UnexpectedObjectType { err })?;
        Ok(tree)
    }

    /// Resolves revision string to git object id (Sha1Hash)
    pub fn resolve_revision_str(&mut self, spec: &str) -> GitResult<Sha1Hash> {
        let repo = self.repo()?;
        let obj = repo.revparse_single(spec)
            .map_err(|err| GitErrorDetail::RevisionNotFound {err})?;
        Ok(obj.id().into())
    }

    /// Returns last commit or `None` if repository have no commits (eg. new repository).
    pub fn find_last_commit(&mut self) -> GitResult<Option<Commit>> {
        let repo  = self.repo()?;

        let obj = match repo.head() {
            Ok(head) => head,
            Err(err) => match err.code() {
                ErrorCode::UnbornBranch => return Ok(None),
                _ => {
                    return Err(GitErrorDetail::FindObject {err}).into_diag();
                }
            },
        };

        let obj = obj.resolve().map_err(|err| GitErrorDetail::ResolveReference {err})?;
        let commit = obj.peel_to_commit().map_err(|err| GitErrorDetail::UnexpectedObjectType {err})?;

        Ok(Some(commit))
    }


    /// Creates new git repository
    pub fn init_new_repository<P: AsRef<Path>>(path: P, opts: &RepositoryInitOptions) -> GitResult<()> {
        let _repo = Repository::init_opts(path.as_ref(), opts)
            .map_err(|err| GitErrorDetail::CreateRepository {err})
            .into_diag()?;
        Ok(())
    }

}
