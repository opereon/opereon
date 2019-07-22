



use git2::{Commit, Index, IndexAddOption, ObjectType, Oid, Repository, RepositoryInitOptions, Signature, ErrorCode};

use super::*;
use crate::{ConfigRef};
use kg_utils::collections::LruCache;
use std::path::{PathBuf, Path};
use op_model::{Sha1Hash, ModelRef, DEFAULT_MANIFEST_FILENAME};
use slog::{Record, Serializer, Key, Result as SlogResult};

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

            match fs::metadata(manifest){
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
            return Ok(())
        }

        debug!(self.logger, "Initializing model manager");
        let model_dir = Self::search_manifest(&self.model_dir)?;
        info!(self.logger, "Model dir found {}", model_dir.display());
        self.model_dir = model_dir;
        self.initialized = true;

        Ok(())
    }

    /// Creates new model. Initializes git repository, manifest file etc.
    pub fn init_model(&mut self) -> IoResult<()> {

        let current_dir = fs::current_dir()?;

        // TODO error handling
        Self::init_git_repo(&current_dir)?;
        Self::init_manifest(&current_dir)?;
        Self::init_operc(&current_dir)?;

        Ok(())
    }

    /// Commit current model
    pub fn commit(&mut self, message: &str) -> IoResult<ModelRef> {
        self.init()?;
        // TODO ws error handling
        let repo = Repository::open(self.model_dir()).expect("Cannot open repository");
        let mut index = repo.index().expect("Cannot get index!");

        let oid = Self::update_index(&mut index)?;
        let parent = Self::find_last_commit(&repo)?;
        let tree = repo.find_tree(oid).expect("Cannot get tree!");
        let signature = Signature::now("opereon", "example@email.com").unwrap();

        if let Some(parent) = parent  {
            let _commit = repo.commit(Some("HEAD"),
                                      &signature,
                                      &signature,
                                      message,
                                      &tree,
                                      &[&parent]).expect("Cannot commit model!");
        } else {
            let _commit = repo.commit(Some("HEAD"),
                                      &signature,
                                      &signature,
                                      message,
                                      &tree,
                                      &[]).expect("Cannot commit model!");
        };

        repo.checkout_index(None, None).expect("Cannot checkout index");

        self.get(oid.into())

    }

    pub fn get(&mut self, id: Sha1Hash) -> IoResult<ModelRef> {
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

    pub fn resolve(&mut self, model_path: &ModelPath) -> IoResult<ModelRef> {
        self.init()?;
        match *model_path {
            ModelPath::Current => self.current(),
            ModelPath::Revision(ref rev) => self.get(self.resolve_revision_str(rev)?),
            ModelPath::Path(ref _path) => unimplemented!(),
        }
    }

    /// Returns current model - model represented by content of the git index.
    /// This method loads model on each call.
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

    /// Returns last commit or `None` if repository have no commits.
    fn find_last_commit(repo: &Repository) -> IoResult<Option<Commit>> {
        // TODO error handling
        let obj = match repo.head() {
            Ok(head) => head,
            Err(err) => {
                match err.code() {
                    ErrorCode::UnbornBranch => {
                        return Ok(None)
                    },
                    _=> {
                        eprintln!("err = {:?}", err);
                        panic!("Error searching last commit")
                    }
                }
            },
        };




        let obj = obj.resolve().unwrap().peel(ObjectType::Commit).unwrap();
        let commit = obj.peel_to_commit().unwrap();
        Ok(Some(commit))
    }


    fn init_git_repo<P: AsRef<Path>>(path: P) -> IoResult<()> {
        use std::fmt::Write;
        let mut opts = RepositoryInitOptions::new();
        opts.no_reinit(true);
        // TODO error handling
        let _repo = Repository::init_opts(path.as_ref(), &opts).expect("Cannot create git repository!");

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


    fn resolve_revision_str(&self, spec: &str) -> IoResult<Sha1Hash> {
        // TODO ws error handling
        let repo = Repository::open(self.model_dir()).expect("Cannot open repository");
        let obj = repo.revparse_single(spec).expect("Cannot find revision!");
        Ok(obj.id().into())
    }

    fn cache_model(&mut self, m: ModelRef) {
        let id = m.lock().metadata().id();
        self.model_cache.insert(id, m);
    }

    fn model_dir(&self) -> &Path {
        &self.model_dir
    }
}

#[cfg(test)]
mod tests {
    use git2::{DiffFindOptions, DiffFormat, DiffOptions, Index, IndexAddOption, ObjectType, Oid};
    use git2::build::CheckoutBuilder;

    use super::*;

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
