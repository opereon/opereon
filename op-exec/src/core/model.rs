use git2::{RepositoryInitOptions, Signature};

use super::*;
use crate::ConfigRef;
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
    pub fn search_manifest(start_dir: &Path) -> IoResult<PathBuf> {
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
                        return Err(err);
                    } else {
                        parent = curr_dir.parent();
                    }
                }
            }
        }

        return Err(IoError::file_not_found(manifest_filename, OpType::Read));
    }

    /// Initialize model manager.
    /// This method can be called multiple times.
    pub fn init(&mut self) -> IoResult<()> {
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
    pub fn init_model(&mut self) -> RuntimeResult<()> {
        let current_dir = fs::current_dir()?;

        Self::init_git_repo(&current_dir)?;
        Self::init_manifest(&current_dir)?;
        Self::init_operc(&current_dir)?;

        Ok(())
    }

    /// Commit current model
    pub fn commit(&mut self, message: &str) -> RuntimeResult<ModelRef> {
        self.init()?;

        let git = GitManager::new(self.model_dir())?;
        let signature = Signature::now("opereon", "example@email.com").unwrap();

        let oid = git.commit(message, &signature)?;

        self.get(oid.into())
    }

    pub fn get(&mut self, id: Sha1Hash) -> RuntimeResult<ModelRef> {
        self.init()?;
        if let Some(b) = self.model_cache.get_mut(&id) {
            return Ok(b.clone());
        }

        let mut meta = Metadata::default();

        meta.set_id(id);
        meta.set_path(self.model_dir().to_owned());

        let model = ModelRef::read(meta)?;
        self.cache_model(model.clone());
        Ok(model)
    }

    pub fn resolve(&mut self, model_path: &ModelPath) -> RuntimeResult<ModelRef> {
        self.init()?;
        match *model_path {
            ModelPath::Current => self.current(),
            ModelPath::Revision(ref rev) => self.get(self.resolve_revision_str(rev)?),
            ModelPath::Path(ref _path) => unimplemented!(),
        }
    }

    /// Returns current model - model represented by content of the git index.
    /// This method loads model on each call.
    pub fn current(&mut self) -> RuntimeResult<ModelRef> {
        let oid = GitManager::new(self.model_dir())?.update_index()?;
        self.get(oid.into())
    }

    fn init_git_repo<P: AsRef<Path>>(path: P) -> RuntimeResult<()> {
        use std::fmt::Write;
        let mut opts = RepositoryInitOptions::new();
        opts.no_reinit(true);

        GitManager::init_new_repository(&path, &opts)?;

        // ignore ./op directory
        let excludes = path.as_ref().join(PathBuf::from(".git/info/exclude"));
        let mut content = fs::read_string(&excludes)?;
        writeln!(&mut content, "# Opereon tmp directory")?;
        writeln!(&mut content, ".op/")?;
        fs::write(excludes, content)?;
        Ok(())
    }

    fn init_manifest<P: AsRef<Path>>(path: P) -> IoResult<()> {
        use std::fmt::Write;

        // ignore ./op directory
        let manifest_path = path.as_ref().join(PathBuf::from("op.toml"));
        let mut content = String::new();
        writeln!(&mut content, "[info]")?;
        writeln!(&mut content, "authors = [\"\"]")?;
        writeln!(&mut content, "description = \"Opereon model\"")?;
        fs::write(manifest_path, content)?;
        Ok(())
    }

    fn init_operc<P: AsRef<Path>>(path: P) -> IoResult<()> {
        use std::fmt::Write;

        // ignore ./op directory
        let manifest_path = path.as_ref().join(PathBuf::from(".operc"));
        let mut content = String::new();
        writeln!(&mut content, "[[exclude]]")?;
        writeln!(&mut content, "path = \".op\"")?;
        fs::write(manifest_path, content)?;
        Ok(())
    }

    fn resolve_revision_str(&self, spec: &str) -> RuntimeResult<Sha1Hash> {
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
