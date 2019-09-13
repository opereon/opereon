use git2::RepositoryInitOptions;

use self::ModelManagerErrorDetail::*;
use super::*;
use crate::ConfigRef;
use kg_diag::{BasicDiag, DiagResultExt, IntoDiagRes};
use kg_utils::collections::LruCache;
use op_model::{ModelRef, DEFAULT_MANIFEST_FILENAME};
use slog::{Key, Record, Result as SlogResult, Serializer};
use std::path::{Path, PathBuf};
use parking_lot::ReentrantMutex;
use std::ops::DerefMut;


pub type ModelManagerError = BasicDiag;
pub type ModelManagerResult<T> = Result<T, ModelManagerError>;

#[derive(Debug, Display, Detail)]
pub enum ModelManagerErrorDetail {
    #[display(fmt = "cannot init manifest file in '{path}'")]
    InitManifest { path: String },

    #[display(fmt = "cannot init git repository in '{path}'")]
    InitRepo { path: String },

    #[display(fmt = "cannot init .operc file in '{path}'")]
    InitOperc { path: String },
}

#[derive(Debug)]
pub struct ModelManager {
    config: ConfigRef,
    model_cache: LruCache<Oid, ModelRef>,
    repo_path: PathBuf,
    repo_manager: Option<Box<dyn FileVersionManager>>,
    logger: slog::Logger,
}

impl ModelManager {
    pub fn new(repo_path: PathBuf, config: ConfigRef, logger: slog::Logger) -> ModelManager {
        let model_cache = LruCache::new(config.model().cache_limit());

        ModelManager {
            config,
            model_cache,
            repo_path,
            repo_manager: None,
            logger,
        }
    }

    /*/// Creates new model. Initializes git repository, manifest file etc.
    pub fn init_model<P: AsRef<Path>>(&self, path: P) -> ModelManagerResult<()> {
        self.init_git_repo(&path).map_err_as_cause(|| InitRepo {
            path: path.as_ref().to_string_lossy().to_string(),
        })?;
        self.init_manifest(&path).map_err_as_cause(|| InitManifest {
            path: path.as_ref().to_string_lossy().to_string(),
        })?;
        self.init_operc(&path).map_err_as_cause(|| InitOperc {
            path: path.as_ref().to_string_lossy().to_string(),
        })?;
        Ok(())
    }*/

    /// Commit current model
    pub fn commit(&mut self, message: &str) -> ModelManagerResult<ModelRef> {
        self.init()?;

        let oid = self.repo_manager_mut().commit(message)?;
        self.get(oid)
    }

    pub fn get(&mut self, id: Oid) -> ModelManagerResult<ModelRef> {
        self.init()?;

        if let Some(b) = self.model_cache.get_mut(&id) {
            return Ok(b.clone());
        }

        let rev_info = self.repo_manager_mut().checkout(id)?;
        let model = ModelRef::read(rev_info, self.logger.clone())?;
        self.cache_model(model.clone());
        Ok(model)
    }

    pub fn resolve(&mut self, rev_path: &RevPath) -> ModelManagerResult<ModelRef> {
        self.init()?;

        let oid = self.repo_manager_mut().resolve(rev_path)?;
        self.get(oid)
    }

    /// Returns current model
    pub fn current(&mut self) -> ModelManagerResult<ModelRef> {
        //let oid = GitManager::new(self.model_dir())?.update_index()?;
        self.get(Oid::nil())
    }

    pub fn get_file_diff(&mut self, old_rev: &RevPath, new_rev: &RevPath) -> ModelManagerResult<FileDiff> {
        self.init()?;

        let mut repo_manager = self.repo_manager_mut();
        let old_id = repo_manager.resolve(old_rev)?;
        let new_id = repo_manager.resolve(new_rev)?;
        repo_manager.get_file_diff(old_id, new_id)
    }

    pub fn create_model(&mut self, repo_path: PathBuf) -> ModelManagerResult<ModelRef> {
        let repo_manager = op_rev::create_repository(&repo_path)?;

        let logger = self.logger.new(o!("repo_path" => repo_path.display().to_string()));
        self.logger = logger;

        info!(self.logger, "created repository {}", repo_path.display());
        self.repo_path = repo_path;
        self.repo_manager = Some(repo_manager);

        let rev_info = RevInfo::new(Oid::nil(), self.repo_path.clone());
        let model = ModelRef::create(rev_info, self.logger.clone())?;
        self.cache_model(model.clone());
        Ok(model)
    }

    fn init(&mut self) -> ModelManagerResult<()> {
        if self.repo_manager.is_some() {
            return Ok(());
        }

        let repo_path = Model::resolve_manifest_dir(&self.repo_path)?;
        let logger = self.logger.new(o!("repo_path" => repo_path.display().to_string()));
        self.logger = logger;

        info!(self.logger, "opened repository {}", repo_path.display());
        self.repo_path = repo_path;
        self.repo_manager = Some(op_rev::open_repository(&self.repo_path)?);

        Ok(())
    }

    fn cache_model(&mut self, m: ModelRef) {
        let id = m.lock().rev_info().id();
        self.model_cache.insert(id, m);
    }

    fn repo_manager_mut(&mut self) -> &mut dyn FileVersionManager {
        self.repo_manager.as_mut().unwrap().deref_mut()
    }
}
