use super::*;

use std::ops::DerefMut;

pub type ModelManagerResult<T> = Result<T, BasicDiag>;

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

    /// Commit current model
    pub fn commit(&mut self, message: &str) -> ModelManagerResult<Oid> {
        self.init()?;

        let oid = self.repo_manager_mut().commit(message)?;
        Ok(oid)
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
        self.resolve(&RevPath::Current)
    }

    pub fn get_file_diff(&mut self, old_rev: &RevPath, new_rev: &RevPath) -> ModelManagerResult<FileDiff> {
        self.init()?;

        let repo_manager = self.repo_manager_mut();
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
