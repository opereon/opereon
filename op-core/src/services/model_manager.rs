use crate::config::ModelConfig;
use kg_diag::BasicDiag;
use kg_utils::collections::LruCache;
use op_model::{Model, ModelRef};
use op_rev::{FileDiff, FileVersionManager, Oid, RevInfo, RevPath};
use std::ops::DerefMut;
use std::path::PathBuf;

pub type ModelManagerResult<T> = Result<T, BasicDiag>;

#[derive(Debug)]
pub struct ModelManager {
    config: ModelConfig,
    model_cache: LruCache<Oid, ModelRef>,
    repo_path: PathBuf,
    repo_manager: Option<Box<dyn FileVersionManager + Send>>,
}

impl ModelManager {
    pub fn new(repo_path: PathBuf, config: ModelConfig) -> ModelManager {
        let model_cache = LruCache::new(config.cache_limit());

        ModelManager {
            config,
            model_cache,
            repo_path,
            repo_manager: None,
        }
    }

    /// Commit current model
    pub async fn commit(&mut self, message: &str) -> ModelManagerResult<Oid> {
        self.init_model().await?;
        let oid = self.repo_manager_mut().commit(message).await?;
        Ok(oid)
    }

    pub async fn get(&mut self, id: Oid) -> ModelManagerResult<ModelRef> {
        self.init_model().await?;

        if let Some(b) = self.model_cache.get_mut(&id) {
            return Ok(b.clone());
        }

        let rev_info = self.repo_manager_mut().checkout(id).await?;
        let model = ModelRef::read(rev_info)?;
        self.cache_model(model.clone());
        Ok(model)
    }

    pub async fn resolve(&mut self, rev_path: &RevPath) -> ModelManagerResult<ModelRef> {
        self.init_model().await?;

        let oid = self.repo_manager_mut().resolve(rev_path).await?;
        self.get(oid).await
    }

    /// Returns current model
    pub async fn current(&mut self) -> ModelManagerResult<ModelRef> {
        self.resolve(&RevPath::Current).await
    }

    pub async fn get_file_diff(
        &mut self,
        old_rev: &RevPath,
        new_rev: &RevPath,
    ) -> ModelManagerResult<FileDiff> {
        self.init_model().await?;

        let repo_manager = self.repo_manager_mut();
        let old_id = repo_manager.resolve(old_rev).await?;
        let new_id = repo_manager.resolve(new_rev).await?;
        repo_manager.get_file_diff(old_id, new_id).await
    }

    #[instrument(skip(self))]
    pub async fn create_model(&mut self, repo_path: PathBuf) -> ModelManagerResult<ModelRef> {
        let repo_manager = op_rev::create_repository(&repo_path).await?;

        self.repo_path = repo_path;
        self.repo_manager = Some(repo_manager);

        let rev_info = RevInfo::new(Oid::nil(), self.repo_path.clone());
        let model = ModelRef::create(rev_info)?;
        info!(verb=2, "Repository created");
        self.cache_model(model.clone());
        Ok(model)
    }

    #[instrument(skip(self))]
    async fn init_model(&mut self) -> ModelManagerResult<()> {
        if self.repo_manager.is_some() {
            return Ok(());
        }

        let repo_path = Model::resolve_manifest_dir(&self.repo_path)?;

        self.repo_path = repo_path;
        self.repo_manager = Some(op_rev::open_repository(&self.repo_path).await?);
        info!(verb=2, repo_path=?self.repo_path, "Repository opened");

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
