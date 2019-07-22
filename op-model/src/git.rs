use crate::Sha1Hash;
use git2::{Repository, Tree};
use kg_diag::BasicDiag;
use kg_diag::Severity;
use std::path::{Path, PathBuf};

pub type GitError = BasicDiag;
pub type GitResult<T> = Result<T, GitError>;

#[derive(Debug, Display, Detail)]
#[diag(code_offset = 1100)]
pub enum GitErrorDetail {
    #[display(fmt = "cannot open git repository: '{err}'")]
    OpenRepository { err: git2::Error },

    #[display(fmt = "cannot get git object database: '{err}'")]
    GetDatabase { err: git2::Error },

    #[display(fmt = "cannot find git object: '{err}'")]
    FindObject { err: git2::Error },

    #[display(fmt = "unexpected git object type: '{err}'")]
    UnexpectedObjectType { err: git2::Error },
}

pub struct GitManager {
    repo_dir: PathBuf,
    repo: Option<Repository>,
}

impl GitManager {
    pub fn new<P: AsRef<Path>>(repo_dir: P) -> Self {
        Self {
            repo_dir: repo_dir.as_ref().to_path_buf(),
            repo: None,
        }
    }

    fn open(&mut self) -> GitResult<&mut Repository> {
        if let Some(ref mut repo) = self.repo {
            return Ok(repo);
        }

        let repo: Repository = Repository::open(&self.repo_dir)
            .map_err(|err| BasicDiag::from(GitErrorDetail::OpenRepository { err }))?;
        self.repo = Some(repo);
        Ok(self.repo.as_mut().unwrap())
    }

    pub fn get_commit_tree(&mut self, commit: &Sha1Hash) -> GitResult<Tree> {
        let repo = self.open()?;
        let odb = repo
            .odb()
            .map_err(|err| GitErrorDetail::GetDatabase { err })?;

        let obj = repo
            .find_object(commit.as_oid(), None)
            .map_err(|err| GitErrorDetail::FindObject { err })?;

        let commit_tree = obj
            .peel_to_tree()
            .map_err(|err| GitErrorDetail::UnexpectedObjectType { err })?;
        Ok(commit_tree)
    }
}
