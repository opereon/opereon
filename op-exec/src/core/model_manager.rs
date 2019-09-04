use git2::RepositoryInitOptions;

use self::ModelManagerErrorDetail::*;
use super::*;
use crate::ConfigRef;
use kg_diag::{BasicDiag, DiagResultExt, IntoDiagRes};
use kg_utils::collections::LruCache;
use op_model::{ModelRef, Sha1Hash, DEFAULT_MANIFEST_FILENAME};
use slog::{Key, Record, Result as SlogResult, Serializer};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type", content = "arg")]
pub enum ModelPath {
    /// Current working directory
    Current,
    /// Git revision string http://git-scm.com/docs/git-rev-parse.html#_specifying_revisions
    Revision(String),
    /// Path to model directory. Currently unimplemented.
    Path(PathBuf),
}

impl std::fmt::Display for ModelPath {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            ModelPath::Current => write!(f, "@"),
            ModelPath::Revision(ref id) => write!(f, "id: {}", id),
            ModelPath::Path(ref path) => write!(f, "{}", path.display()),
        }
    }
}

impl std::str::FromStr for ModelPath {
    type Err = String;

    fn from_str(s: &str) -> Result<ModelPath, Self::Err> {
        Ok(match s {
            "@" | "@current" => ModelPath::Current,
            _ => ModelPath::Revision(s.to_string()),
        })
    }
}

impl slog::Value for ModelPath {
    fn serialize(&self, _record: &Record, key: Key, serializer: &mut dyn Serializer) -> SlogResult {
        serializer.emit_str(key, &format!("{}", &self))
    }
}

pub type ModelManagerError = BasicDiag;
pub type ModelManagerResult<T> = Result<T, ModelManagerError>;

#[derive(Debug, Display, Detail)]
pub enum ModelManagerErrorDetail {
    #[display(fmt = "manifest file not found")]
    ManifestNotFound,

    #[display(fmt = "cannot find manifest file")]
    ManifestSearch,

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
    model_cache: LruCache<Sha1Hash, ModelRef>,
    /// Path do model dir.
    model_dir: PathBuf,
    initialized: bool,
    logger: slog::Logger,
}

impl ModelManager {
    pub fn new(model_dir: PathBuf, config: ConfigRef, logger: slog::Logger) -> ModelManager {
        let model_cache = LruCache::new(config.model().cache_limit());
        let logger = logger.new(o!("model_dir" => model_dir.display().to_string()));
        ModelManager {
            config,
            model_cache,
            model_dir,
            initialized: false,
            logger,
        }
    }

    /// Travels up the directory structure to find first occurrence of `op.toml` manifest file.
    /// # Returns
    /// path to manifest dir.
    pub fn search_manifest(start_dir: &Path) -> ModelManagerResult<PathBuf> {
        let manifest_filename = PathBuf::from(DEFAULT_MANIFEST_FILENAME);

        let mut parent = Some(start_dir);

        while parent.is_some() {
            let curr_dir = parent.unwrap();
            let manifest = curr_dir.join(&manifest_filename);

            match fs::metadata(manifest) {
                Ok(_) => {
                    return Ok(curr_dir.to_owned());
                }
                Err(err) => {
                    if err.kind() != std::io::ErrorKind::NotFound {
                        return Err(err)
                            .into_diag_res()
                            .map_err_as_cause(|| ModelManagerErrorDetail::ManifestSearch);
                    } else {
                        parent = curr_dir.parent();
                    }
                }
            }
        }

        Err(ModelManagerErrorDetail::ManifestNotFound.into())
    }

    /// Initialize model manager.
    /// This method can be called multiple times.
    pub fn init(&mut self) -> ModelManagerResult<()> {
        if self.initialized {
            return Ok(());
        }

        debug!(self.logger, "Initializing model manager");
        let model_dir = Self::search_manifest(&self.model_dir)?;
        info!(self.logger, "Model dir found {}", model_dir.display());
        self.model_dir = model_dir;
        self.initialized = true;

        Ok(())
    }

    /// Creates new model. Initializes git repository, manifest file etc.
    pub fn init_model<P: AsRef<Path>>(path: P) -> ModelManagerResult<()> {
        Self::init_git_repo(&path).map_err_as_cause(|| InitRepo {
            path: path.as_ref().to_string_lossy().to_string(),
        })?;
        Self::init_manifest(&path).map_err_as_cause(|| InitManifest {
            path: path.as_ref().to_string_lossy().to_string(),
        })?;
        Self::init_operc(&path).map_err_as_cause(|| InitOperc {
            path: path.as_ref().to_string_lossy().to_string(),
        })?;
        Ok(())
    }

    /// Commit current model
    pub fn commit(&mut self, message: &str) -> ModelManagerResult<ModelRef> {
        self.init()?;

        let git = GitManager::new(self.model_dir())?;

        let oid = git.commit(message)?;

        self.get(oid)
    }

    pub fn get(&mut self, id: Sha1Hash) -> ModelManagerResult<ModelRef> {
        self.init()?;
        if let Some(b) = self.model_cache.get_mut(&id) {
            return Ok(b.clone());
        }

        let mut meta = Metadata::default();

        meta.set_id(id);
        meta.set_path(self.model_dir().to_owned());

        let model = ModelRef::read(meta, self.logger.clone())?;
        self.cache_model(model.clone());
        Ok(model)
    }

    pub fn resolve(&mut self, model_path: &ModelPath) -> ModelManagerResult<ModelRef> {
        self.init()?;
        match *model_path {
            ModelPath::Current => self.current(),
            ModelPath::Revision(ref rev) => self.get(self.resolve_revision_str(rev)?),
            ModelPath::Path(ref _path) => unimplemented!(),
        }
    }

    /// Returns current model - model represented by content of the git index.
    /// This method loads model on each call.
    pub fn current(&mut self) -> ModelManagerResult<ModelRef> {
        let oid = GitManager::new(self.model_dir())?.update_index()?;
        self.get(oid.into())
    }

    fn init_git_repo<P: AsRef<Path>>(path: P) -> ModelManagerResult<()> {
        use std::fmt::Write;
        let mut opts = RepositoryInitOptions::new();
        opts.no_reinit(true);

        GitManager::init_new_repository(&path, &opts)?;

        // ignore ./op directory
        let excludes = path.as_ref().join(PathBuf::from(".git/info/exclude"));
        let mut content = fs::read_string(&excludes)?;
        // language=gitignore
        let ignore_content = r#"
# Opereon tmp directory
.op/
"#;
        writeln!(&mut content, "{}", ignore_content).map_err(IoErrorDetail::from)?;
        fs::write(excludes, content)?;
        Ok(())
    }

    fn init_manifest<P: AsRef<Path>>(path: P) -> ModelManagerResult<()> {
        let manifest_path = path.as_ref().join(PathBuf::from("op.toml"));
        // language=toml
        let default_manifest = r#"
[info]
authors = [""]
description = "Opereon model"
"#;
        fs::write(manifest_path, default_manifest)?;
        Ok(())
    }

    fn init_operc<P: AsRef<Path>>(path: P) -> ModelManagerResult<()> {
        let manifest_path = path.as_ref().join(PathBuf::from(".operc"));
        // language=toml
        let default_operc = r#"
[[exclude]]
path = ".op"
"#;
        fs::write(manifest_path, default_operc)?;
        Ok(())
    }

    fn resolve_revision_str(&self, spec: &str) -> ModelManagerResult<Sha1Hash> {
        let git = GitManager::new(self.model_dir())?;
        let id = git.resolve_revision_str(spec)?;
        Ok(id)
    }

    fn cache_model(&mut self, m: ModelRef) {
        let id = m.lock().metadata().id();
        self.model_cache.insert(id, m);
    }

    fn model_dir(&self) -> &Path {
        &self.model_dir
    }
}
