use super::*;

use std::collections::HashMap;

use crypto::sha1::Sha1;
use git2::{Repository, RepositoryInitOptions};
use std::sync::{Mutex, Arc, RwLock};
use std::fmt::Formatter;


#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type", content = "arg")]
pub enum ModelPath {
    Current,
    Id(Sha1Hash),
    Path(PathBuf),
}

impl std::fmt::Display for ModelPath {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            ModelPath::Current => write!(f, "@"),
            ModelPath::Id(id) => write!(f, "id:{}", id),
            ModelPath::Path(ref path) => write!(f, "{}", path.display()),
        }
    }
}

impl std::str::FromStr for ModelPath {
    type Err = String;

    fn from_str(s: &str) -> Result<ModelPath, Self::Err> {
        Ok(match s {
            "@" | "@current" => ModelPath::Current,
            _ => ModelPath::Path(PathBuf::from(s)),
        })
    }
}


#[derive(Debug)]
struct Bin {
    id: Uuid,
    model: ModelRef,
}

impl Bin {
    fn new(id: Uuid, model: ModelRef) -> Bin {
        Bin {
            id,
            model,
        }
    }
}


#[derive(Debug)]
struct Bins(Vec<Bin>);

impl Bins {
    fn get(&mut self, bin_id: Uuid) -> ModelRef {
        for b in self.0.iter_mut().skip(1) {
            if b.id == bin_id {
                if b.id.is_nil() {
                    b.model.reset();
                }
                return b.model.clone();
            } else if b.id.is_nil() || b.model.ref_count() == 1 {
                b.id = bin_id;
                b.model.reset();
                return b.model.clone();
            }
        }
        let m = self.0.first().unwrap().model.deep_copy();
        self.0.push(Bin::new(bin_id, m.clone()));
        m
    }

    fn any(&self) -> &ModelRef {
        &self.0.first().unwrap().model
    }
}

impl From<Bin> for Bins {
    fn from(bin: Bin) -> Self {
        Bins(vec![bin])
    }
}

impl From<ModelRef> for Bins {
    fn from(model: ModelRef) -> Self {
        Bins(vec![Bin::new(Uuid::nil(), model)])
    }
}

type RepositoryRef = Arc<Mutex<Repository>>;

pub struct ModelManager {
    config: ConfigRef,
    model_cache: LruCache<Sha1Hash, Bins>,
    path_map: HashMap<PathBuf, Sha1Hash>,
    current: Sha1Hash,
    repository: Option<RepositoryRef>,
    logger: slog::Logger,
}

impl ModelManager {
    pub fn new(config: ConfigRef, logger: slog::Logger) -> ModelManager {
        let model_cache = LruCache::new(config.model().cache_limit());
        ModelManager {
            config,
            model_cache,
            path_map: HashMap::new(),
            current: Sha1Hash::nil(),
            repository: None,
            logger
        }
    }

    fn config(&self) -> &ModelConfig {
        self.config.model()
    }

    pub fn init(&mut self) -> IoResult<()> {
        debug!(self.logger, "Initializing model manager");
        use std::str::FromStr;
//
//        let current_dir = fs::current_dir()?;
//
//        self.repository = match Repository::discover(&current_dir) {
//            Ok(repository) => {
//                debug!(self.logger, "Git repository found in path {}", repository.path().display());
//                Some(Arc::new(Mutex::new(repository)))
//            },
//            Err(err) => {
//                warn!(self.logger, "Git repository not found! {:?}", err);
//                None
//            }
//        };

        kg_io::fs::create_dir_all(self.config().data_dir())?;

        let current_file_path = self.config().data_dir().join("current");
        if current_file_path.exists() {
            let mut current = String:: new();
            kg_io::fs::read_to_string(&current_file_path, &mut current)?;
            match Sha1Hash::from_str(&current) {
                Ok(id) => {
                    let m = self.get(id)?;
                    self.current = id;
                }
                Err(_err) => return Err(std::io::ErrorKind::InvalidData.into()),
            }
        } else {
            let m = ModelRef::default();
            self.cache_model(Bin::new(Uuid::nil(), m));
        }

        Ok(())
    }

    pub fn init_model(&mut self) -> IoResult<()>{

        let mut opts = RepositoryInitOptions::new();
        opts.no_reinit(true);

        // TODO error handling
        let repo = Repository::init_opts(fs::current_dir()?, &opts).expect("Cannot create git repository!");
//        repo.add_ignore_rule(".op").unwrap();

        self.repository = Some(Arc::new(Mutex::new(repo)));

        Ok(())
    }

    pub fn store<P: AsRef<Path>>(&mut self, metadata: Metadata, path: P) -> IoResult<ModelRef> {
        debug!(self.logger, "Saving new model"; o!("source_path"=> path.as_ref().display()));

        let (path, _) =  Model::search_manifest(path.as_ref())?;

        let mut sha1 = Sha1::new();

        let tmp_model_dir = self.config().data_dir().join(Uuid::new_v4().to_string());
        let tmp_files_dir = tmp_model_dir.join("files");
        std::fs::create_dir_all(&tmp_files_dir)?;

        let id = Model::copy(&path, &tmp_files_dir)?;
        let model_dir = self.config().data_dir().join(id.to_string());
        let files_dir = model_dir.join("files");

        std::fs::rename(tmp_model_dir, &model_dir)?;
        std::fs::write(model_dir.join("_model.yaml"), serde_yaml::to_string(&metadata).unwrap())?;

        let model = ModelRef::read(metadata, &files_dir)?;
        model.lock().metadata_mut().set_stored(true);
        self.cache_model(Bin::new(Uuid::nil(), model.clone()));

        Ok(model)
    }

    pub fn get(&mut self, id: Sha1Hash) -> IoResult<ModelRef> {
        self.get_bin(id, Uuid::nil())
    }

    pub fn get_bin(&mut self, id: Sha1Hash, bin_id: Uuid) -> IoResult<ModelRef> {
        if let Some(b) = self.model_cache.get_mut(&id) {
            return Ok(b.get(bin_id));
        }
        let model_dir = self.config().data_dir().join(id.to_string());
        let s = std::fs::read_to_string(model_dir.join("_model.yaml"))?;
        let mut meta: Metadata = serde_yaml::from_str(&s).unwrap(); //FIXME
        meta.set_stored(true);

        let model = ModelRef::read(meta, &model_dir.join("files"))?;
        self.cache_model(Bin::new(bin_id, model.clone()));
        Ok(model)
    }

    pub fn read(&mut self, path: &Path) -> IoResult<ModelRef> {
        self.read_bin(path, Uuid::nil())
    }

    pub fn read_bin(&mut self, path: &Path, bin_id: Uuid) -> IoResult<ModelRef> {
        if let Some(&id) = self.path_map.get(path) {
            return self.get_bin(id, bin_id);
        }
        let model = ModelRef::read(Metadata::default(), path)?;
        self.cache_model(Bin::new(bin_id, model.clone()));
        Ok(model)
    }

    pub fn resolve(&mut self, model_path: &ModelPath) -> IoResult<ModelRef> {
        self.resolve_bin(model_path, Uuid::nil())
    }

    pub fn resolve_bin(&mut self, model_path: &ModelPath, bin_id: Uuid) -> IoResult<ModelRef> {
        match *model_path {
            ModelPath::Current => self.current_bin(bin_id),
            ModelPath::Id(id) => self.get_bin(id, bin_id),
            ModelPath::Path(ref path) => self.read_bin(path, bin_id),
        }
    }

    pub fn list(&self) -> std::io::Result<Vec<Metadata>> {
        use std::fs::{read_dir, read_to_string};

        let mut list = Vec::new();

        let model_dir = self.config().data_dir();
        for e in read_dir(model_dir)? {
            let e = e?;
            if e.path().is_dir() {
                let s = read_to_string(e.path().join("_model.yaml"))?;
                let meta: Metadata = serde_yaml::from_str(&s).unwrap(); //FIXME
                list.push(meta);
            }
        }

        list.sort_by(|a, b| a.timestamp().cmp(&b.timestamp()));
        Ok(list)
    }

    pub fn current(&mut self) -> IoResult<ModelRef> {
        let id = self.current;
        self.get(id)
    }

    pub fn current_bin(&mut self, bin_id: Uuid) -> IoResult<ModelRef> {
        let id = self.current;
        self.get_bin(id, bin_id)
    }

    pub fn set_current(&mut self, model: ModelRef) -> IoResult<()> {
        assert!(model.lock().metadata().is_stored());
        let id = model.lock().metadata().id();
        self.current = id;
        let current_file_path = self.config().data_dir().join("current");
        kg_io::fs::write(current_file_path, id.to_string())?;
        self.cache_model(Bin::new(Uuid::nil(), model));
        info!(self.logger, "Current model changed to {}", id);
        Ok(())
    }

    fn cache_model(&mut self, bin: Bin) {
        let id = bin.model.lock().metadata().id();
        self.model_cache.insert(id, bin.into());
        self.path_map.clear();
        for (&id, b) in self.model_cache.iter() {
            self.path_map.insert(b.any().lock().metadata().path().to_owned(), id);
        }
    }
}

impl std::fmt::Debug for ModelManager {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::build::CheckoutBuilder;
    use git2::ObjectType;

    #[test]
    fn git_diff_test()-> Result<(), git2::Error> {
        let current = std::env::current_dir().unwrap();
        let out_dir = current.join(".op/checked_out");

        fs::create_dir_all(&out_dir).unwrap();
        let repo = Repository::open(&current).unwrap();

        let mut builder = CheckoutBuilder::new();
        builder.target_dir(&out_dir);

        let mut index = repo.index()?;

        repo.checkout_index(Some(&mut index), Some(&mut builder))
    }
}
