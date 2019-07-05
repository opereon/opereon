use super::*;

use std::collections::HashMap;

use git2::{Repository, RepositoryInitOptions, IndexAddOption, Index, Oid, ObjectType, Signature, Tree, Commit};
use std::sync::{Mutex, Arc, RwLock};
use std::fmt::Formatter;


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
            ModelPath::Revision(ref id) => write!(f, "id:{}", id),
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

/*
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
*/

#[derive(Debug)]
pub struct ModelManager {
    config: ConfigRef,
    model_cache: LruCache<Sha1Hash, ModelRef>,
    /// Path do model dir.
    model_dir: PathBuf,
    logger: slog::Logger,
}

impl ModelManager {
    pub fn new(model_dir: PathBuf, config: ConfigRef, logger: slog::Logger) -> ModelManager {
        let model_cache = LruCache::new(config.model().cache_limit());
        ModelManager {
            config,
            model_cache,
            model_dir,
            logger,
        }
    }

    fn config(&self) -> &ModelConfig {
        self.config.model()
    }

    fn model_dir(&self) -> &Path {
        &self.model_dir
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

            match kg_io::fs::metadata(manifest){
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

        return Err(kg_io::IoError::file_not_found(manifest_filename, OpType::Read));
    }

    pub fn init(&mut self) -> IoResult<()> {
        debug!(self.logger, "Initializing model manager");
        use std::str::FromStr;

        let model_dir = Self::search_manifest(&self.model_dir)?;
        info!(self.logger, "Model dir found {}", model_dir.display());
        self.model_dir = model_dir;

        Ok(())
    }

    pub fn init_model(&mut self) -> IoResult<()> {

        let current_dir = fs::current_dir()?;

        // TODO error handling
        Self::init_git_repo(&current_dir)?;
        Self::init_manifest(&current_dir)?;

        Ok(())
    }

    fn init_git_repo<P: AsRef<Path>>(path: P) -> IoResult<()> {
        use std::fmt::Write;
        let mut opts = RepositoryInitOptions::new();
        opts.no_reinit(true);
        // TODO error handling
        let repo = Repository::init_opts(path.as_ref(), &opts).expect("Cannot create git repository!");

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

    /// Commit current model
    pub fn commit(&mut self, message: &str) -> IoResult<ModelRef> {
        // TODO ws error handling
        let repo = Repository::open(self.model_dir()).expect("Cannot open repository");
        let mut index = repo.index().expect("Cannot get index!");

        let oid = Self::update_index(&mut index)?;
        let parent = Self::find_last_commit(&repo)?;
        let tree = repo.find_tree(oid).expect("Cannot get tree!");
        let signature = Signature::now("opereon", "example@email.com").unwrap();

        let commit = repo.commit(Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &[&parent]).expect("Cannot commit model!");

        repo.checkout_index(None, None).unwrap();

        self.get(oid.into())

    }

    pub fn get(&mut self, id: Sha1Hash) -> IoResult<ModelRef> {
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

    pub fn resolve(&mut self, model_path: &ModelPath) -> IoResult<ModelRef> {
        match *model_path {
            ModelPath::Current => self.current(),
            ModelPath::Revision(ref rev) => self.get(self.resolve_revision_str(rev)?),
            ModelPath::Path(ref path) => unimplemented!(),
        }
    }

    fn resolve_revision_str(&self, spec: &str) -> IoResult<Sha1Hash> {
        // TODO ws error handling
        let repo = Repository::open(self.model_dir()).expect("Cannot open repository");
        let obj = repo.revparse_single(spec).expect("Cannot find revision!");
        Ok(obj.id().into())
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

    /// Returns current model - model represented by content of the git index
    pub fn current(&mut self) -> IoResult<ModelRef> {
        // TODO ws error handling
        let repo = Repository::open(self.model_dir()).expect("Cannot open repository");
        let mut index = repo.index().expect("Cannot get index!");

        let oid = Self::update_index(&mut index)?;

        self.get(oid.into())
    }

    /// Update provided index and return created tree Oid
    fn update_index(index: &mut Index) -> IoResult<Oid> {
        // TODO ws error handling

        // Clear index and rebuild it from working dir. Necessary to reflect .gitignore changes
        // Changes in index won't be saved to disk until index.write*() called.
        index.clear().expect("Cannot clear index");
        index.add_all(&["*"], IndexAddOption::default(), None).expect("Cannot update index");

        // get oid of index tree
        let oid = index.write_tree().expect("Cannot write index");
        Ok(oid)
    }

    fn find_last_commit(repo: &Repository) -> IoResult<Commit> {
        // TODO error handling
        let obj = repo.head().unwrap().resolve().unwrap().peel(ObjectType::Commit).unwrap();
        let commit = obj.peel_to_commit().unwrap();
        Ok(commit)
    }

    fn cache_model(&mut self, m: ModelRef) {
        let id = m.lock().metadata().id();
        self.model_cache.insert(id, m);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::build::CheckoutBuilder;
    use git2::{ObjectType, Oid, DiffOptions, DiffFormat, DiffFindOptions, IndexAddOption, Index};

    #[test]
    fn checkout_to_dir() {
        let current = PathBuf::from("/home/wiktor/Desktop/opereon/resources/model");
        let out_dir = current.join(".op/checked_out");

        let commit_hash = Oid::from_str("996d94321d833a918842c69531197f9d368ec4b6").expect("Cannot parse commit hash");

        let repo = Repository::open(&current).expect("Cannot open repository");

        let commit = repo.find_commit(commit_hash).expect("Cannot find commit");
        let tree = commit.tree().expect("Cannot get commit tree");

        let mut builder = CheckoutBuilder::new();
        builder.target_dir(&out_dir);
        // cannot update current index
        builder.update_index(false);
        // override everything in out_dir with commit state
        builder.force();

        repo.checkout_tree(tree.as_object(), Some(&mut builder)).expect("Cannot checkout tree!");
    }

    #[test]
    fn diff() {
        let current = PathBuf::from("/home/wiktor/Desktop/opereon/resources/model");

        let commit_hash1 = Oid::from_str("6f09d0ad3908daa16992656cb33d4ed075e554a8").expect("Cannot parse commit hash");

        let repo = Repository::open(&current).expect("Cannot open repository");

        let commit1 = repo.find_commit(commit_hash1).expect("Cannot find commit");
        let tree1 = commit1.tree().expect("Cannot get commit tree");

        let mut opts = DiffOptions::new();
        opts.minimal(true);

        let mut index = repo.index().expect("Cannot get index!");

//         TODO what about .operc [[exclude]]? Should it be equal to .gitignore?
        // Clear index and rebuild it from working dir. Necessary to reflect .gitignore changes
        // Changes in index won't be saved to disk until index.write*() called.
        index.clear().expect("Cannot clear index");
        index.add_all(&["*"], IndexAddOption::default(), None).expect("Cannot update index");

//        index.write().expect("cannot write index");

        let mut diff = repo.diff_tree_to_workdir_with_index(Some(&tree1), Some(&mut opts)).expect("Cannot get diff");

        let mut find_opts = DiffFindOptions::new();
        find_opts.renames(true);
        find_opts.renames_from_rewrites(true);
        find_opts.remove_unmodified(true);

        diff.find_similar(Some(&mut find_opts)).expect("Cannot find similar!");
        println!("Diffs:");

        let deltas = diff.deltas();
        eprintln!("deltas.size_hint() = {:?}", deltas.size_hint());
        for delta in deltas {
            println!("======");
            eprintln!("Change type: {:?}", delta.status());
            let old = delta.old_file();
            let new = delta.new_file();
            eprintln!("old = id: {:?}, path: {:?}", old.id(), old.path());
            eprintln!("new = id: {:?}, path: {:?}", new.id(), new.path());
        }
    }
}
