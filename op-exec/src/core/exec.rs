use super::*;

#[derive(Debug)]
pub struct ExecManager {
    config: ConfigRef,
    cache: LruCache<PathBuf, ProcExecRef>,
}

impl ExecManager {
    pub fn new(config: ConfigRef) -> ExecManager {
        let cache = LruCache::new(10); //FIXME (jc) add config param
        ExecManager {
            config,
            cache,
        }
    }

    /*fn config(&self) -> &ModelConfig {
        self.config.model()
    }

    pub fn init(&mut self) -> std::io::Result<()> {
        use std::str::FromStr;

        std::fs::create_dir_all(self.config().data_dir())?;

        let current_file_path = self.config().data_dir().join("current");
        if current_file_path.exists() {
            let current = std::fs::read_to_string(&current_file_path)?;
            match Uuid::from_str(&current) {
                Ok(id) => {
                    let m = self.get(id)?;
                    self.current = m;
                }
                Err(_err) => return Err(std::io::ErrorKind::InvalidData.into()),
            }
        }

        Ok(())
    }

    pub fn store<P: AsRef<Path>>(&mut self, metadata: Metadata, path: P) -> std::io::Result<ModelRef> {
        let model_dir = self.config().data_dir().join(metadata.id().to_string());
        let files_dir = model_dir.join("files");
        std::fs::create_dir_all(&files_dir)?;

        let w = WalkDir::new(&path);
        for er in w.into_iter() {
            match er {
                Ok(e) => {
                    let rel_path = e.path().strip_prefix(&path).unwrap();
                    println!("-> {}", rel_path.display());
                    let dest_path = files_dir.join(rel_path);
                    if !rel_path.as_os_str().is_empty() {
                        if e.file_type().is_dir() {
                            std::fs::create_dir(dest_path)?;
                        } else {
                            std::fs::copy(e.path(), dest_path)?;
                        }
                    }
                }
                Err(err) => {
                    {
                        let path = err.path().unwrap_or(Path::new("")).display();
                        println!("failed to access entry {}", path);
                    }
                    return Err(err.into_io_error().unwrap_or(std::io::ErrorKind::Other.into()));
                }
            }
        }

        let path = std::fs::canonicalize(path)?;
        std::fs::write(model_dir.join("_model.yaml"), serde_yaml::to_string(&metadata).unwrap())?;

        let id = metadata.id();
        let model: ModelRef = Model::read(metadata, &files_dir)?.into();
        self.cache.insert(id, model.clone());

        Ok(model)
    }*/

    pub fn get(&mut self, path: &Path) -> Result<ProcExecRef, ProtoError> {
        if let Some(w) = self.cache.get_mut(path) {
            return Ok(w.clone());
        }
        let e = ProcExec::load(path)?;
        let e = ProcExecRef::new(e);
        self.cache.insert(path.to_owned(), e.clone());
        Ok(e)
    }

    /*pub fn list(&self) -> std::io::Result<Vec<Metadata>> {
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
    }*/
}
