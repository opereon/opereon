use std::any::TypeId;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use git2::{ObjectType, TreeWalkMode, TreeWalkResult};

use parking_lot::{ReentrantMutex, ReentrantMutexGuard};

use super::load_file::LoadFileFunc;
use super::*;
use kg_diag::{BasicDiag, Severity};
use slog::{o, warn, Logger};

const LOAD_FILE_FUNC_NAME: &str = "loadFile";

pub type ModelError = BasicDiag;
pub type ModelResult<T> = Result<T, ModelError>;

#[derive(Debug, Display, Detail)]
#[diag(code_offset = 1000)]
pub enum ModelErrorDetail {
    #[display(fmt = "cannot parse config file: '{file_path}'")]
    MalformedConfigFile { file_path: String },

    #[display(fmt = "cannot load model config")]
    ConfigRead,

    #[display(fmt = "cannot parse manifest file '{file_path}'")]
    MalformedManifest { file_path: String },

    #[display(fmt = "cannot read manifest file")]
    ManifestRead,

    #[display(fmt = "cannot resolve includes")]
    IncludesResolve,

    #[display(fmt = "cannot resolve overrides")]
    OverridesResolve,

    #[display(fmt = "cannot parse defs")]
    DefsParse,

    #[display(fmt = "config file '{file_path}' is not valid utf-8")]
    ConfigUtf8 { file_path: String },

    #[display(fmt = "cannot resolve interpolations")]
    InterpolationsResolve,

    #[display(fmt = "cannot evaluate expression")]
    Expr,

    #[display(fmt = "cannot generate model diff")]
    ModelDiff,
}

#[derive(Debug, Serialize)]
pub struct Model {
    #[serde(flatten)]
    scoped: Scoped,
    metadata: Metadata,
    hosts: Vec<HostDef>,
    users: Vec<UserDef>,
    procs: Vec<ProcDef>,
    #[serde(skip)]
    lookup: ModelLookup,
    #[serde(skip)]
    logger: Logger,
}

impl Model {
    pub fn empty() -> Model {
        let root = NodeRef::object(Properties::new());
        Model {
            metadata: Metadata::default(),
            scoped: Scoped::new(&root, &root, ScopeDef::new()),
            hosts: Vec::new(),
            users: Vec::new(),
            procs: Vec::new(),
            lookup: ModelLookup::new(),
            logger: Logger::root(slog::Discard, o!()),
        }
    }

    pub fn load_manifest(model_dir: &Path) -> ModelResult<Manifest> {
        let path = model_dir.join(PathBuf::from(DEFAULT_MANIFEST_FILENAME));
        let mut content = String::new();
        fs::read_to_string(&path, &mut content)
            .into_diag_res()
            .map_err_as_cause(|| ModelErrorDetail::ManifestRead)?;
        let manifest: Manifest =
            kg_tree::serial::toml::from_str(&content).map_err_as_cause(|| {
                ModelErrorDetail::MalformedManifest {
                    file_path: path.to_string_lossy().to_string(),
                }
            })?;
        Ok(manifest)
    }

    pub fn read_revision(metadata: Metadata, logger: Logger) -> ModelResult<Model> {
        let manifest = Model::load_manifest(metadata.path())?;

        let logger = logger.new(o!("model_id"=> metadata.id().to_string()));

        kg_tree::set_base_path(metadata.path());

        let mut m = Model {
            metadata,
            logger,
            ..Model::empty()
        };

        let cr = ConfigResolver::scan_revision(m.metadata.path(), &m.metadata.id())
            .map_err_as_cause(|| ModelErrorDetail::ConfigRead)?;

        m.root().data_mut().set_file(Some(&FileInfo::new(
            m.metadata.path(),
            FileType::Dir,
            FileFormat::Binary,
        )));

        let scope = ScopeMut::new();

        // TODO
        // Error messages from following functions are not very detailed.
        // Consider more detailed errors. See test cases 'tests/model.rs'
        m.resolve_includes(&cr, &scope)
            .map_err_as_cause(|| ModelErrorDetail::IncludesResolve)?;
        m.set_defines(&manifest);
        m.resolve_overrides(&cr, &scope)
            .map_err_as_cause(|| ModelErrorDetail::OverridesResolve)?;

        // resolve interpolations
        let mut resolver = TreeResolver::new();
        resolver
            .resolve(m.root())
            .map_err_as_cause(|| ModelErrorDetail::InterpolationsResolve)?;

        m.parse_defs()
            .map_err_as_cause(|| ModelErrorDetail::DefsParse)?;

        return Ok(m);
    }

    /// Walk through each entry in model directory, resolve matching `Includes` and apply changes to model tree
    fn resolve_includes(&mut self, cr: &ConfigResolver, scope: &ScopeMut) -> ModelResult<()> {
        let commit = self.metadata.id();
        let model_dir = self.metadata.path().to_owned();

        let git = GitManager::new(&model_dir)?;
        let odb = git.odb()?;
        let tree = git.get_tree(&commit)?;

        let mut walk_err = None;
        tree.walk(TreeWalkMode::PreOrder, |parent_path, entry| {
            let entry_name = entry.name().unwrap();

            let file_type: FileType = match entry.kind().unwrap() {
                ObjectType::Tree => FileType::Dir,
                ObjectType::Blob => FileType::File,
                _ => {
                    warn!(self.logger, "Unknown git object type, skipping : {obj_path}", obj_path = format!("{}/{}", parent_path, entry_name) ;"verbosity" => 0);
                    return TreeWalkResult::Ok;
                }
            };

            if file_type.is_file()
                && (entry_name == DEFAULT_MANIFEST_FILENAME
                    || entry_name == DEFAULT_CONFIG_FILENAME)
            {
                return TreeWalkResult::Ok;
            }

            let path = PathBuf::from_str(parent_path).unwrap().join(entry_name);
            let path_abs = model_dir.join(&path);
            let config = cr.resolve(&path_abs);

            let inner = || -> ModelResult<()> {
                if let Some(inc) = config.find_include(&path, file_type) {
                    let file_info = FileInfo::new(path_abs, file_type, FileFormat::Binary);

                    let obj = odb
                        .read(entry.id())
                        .map_err(|err| GitErrorDetail::GetFile {
                            file: entry.name().unwrap().into(),
                            err,
                        })?;

                    let n = NodeRef::binary(obj.data());
                    n.data_mut().set_file(Some(&file_info));

                    scope.set_func(
                        LOAD_FILE_FUNC_NAME.into(),
                        Box::new(LoadFileFunc::new(model_dir.clone(), PathBuf::from(parent_path), commit)),
                    );

                    let item = inc
                        .item()
                        .apply_one_ext(self.root(), &n, scope.as_ref())
                        .map_err_as_cause(|| ModelErrorDetail::Expr)?;

                    if item.data().file().is_none() {
                        item.data_mut().set_file(Some(&file_info));
                    }

                    scope.set_var("item".into(), NodeSet::One(item));

                    inc.mapping()
                        .apply_ext(self.root(), self.root(), scope.as_ref())
                        .map_err_as_cause(|| ModelErrorDetail::Expr)?;
                    // do not leak temporary scope items
                    scope.remove_var("item");
                    scope.remove_func(LOAD_FILE_FUNC_NAME);
                }
                Ok(())
            };

            if let Err(err) = inner() {
                walk_err = Some(err);
                TreeWalkResult::Abort
            } else {
                TreeWalkResult::Ok
            }
        })
        .map_err(|err| GitErrorDetail::Custom { err })?;
        if let Some(err) = walk_err {
            return Err(err);
        }
        Ok(())
    }

    /// Parse `hosts`, `users` and `procs` definitions
    fn parse_defs(&mut self) -> ModelResult<()> {
        let scope = self.scoped.scope_mut()?;

        // definitions (hosts, users, processors)
        let mut hosts = Vec::new();
        for h in scope.get_var("$hosts").unwrap().iter() {
            let host = HostDef::parse(&self, &self.scoped, h)?;
            hosts.push(host);
        }
        self.hosts = hosts;

        let mut users = Vec::new();
        for u in scope.get_var("$users").unwrap().iter() {
            let user = UserDef::parse(&self, &self.scoped, u)?;
            users.push(user);
        }
        self.users = users;

        let mut procs = Vec::new();
        for p in scope.get_var("$procs").unwrap().iter() {
            let proc = ProcDef::parse(&self, &self.scoped, p)?;
            procs.push(proc);
        }
        self.procs = procs;
        Ok(())
    }

    /// Resolve each `override` and apply changes to model tree.
    fn resolve_overrides(&mut self, cr: &ConfigResolver, scope: &ScopeMut) -> ModelResult<()> {
        for (path, config) in cr.iter() {
            if !config.overrides().is_empty() {
                let path = if path.as_os_str().is_empty() {
                    Opath::parse("@").unwrap()
                } else {
                    Opath::parse(&path.to_str().unwrap().replace('/', ".")).unwrap()
                };

                let node_set = path
                    .apply_ext(self.root(), self.root(), scope.as_ref())
                    .map_err_as_cause(|| ModelErrorDetail::Expr)?;

                let current = if let NodeSet::One(n) = node_set {
                    n
                } else {
                    warn!(self.logger, "Cannot resolve override to single node, assuming model root. Config path: '{path}'", path=path.to_string(); "verbosity"=> 1);
                    self.root().clone()
                };

                scope.set_func(
                    LOAD_FILE_FUNC_NAME.into(),
                    Box::new(LoadFileFunc::new(
                        self.metadata().path().into(),
                        current.data().dir().into(),
                        self.metadata().id(),
                    )),
                );
                for (p, e) in config.overrides().iter() {
                    let res = p
                        .apply_ext(self.root(), &current, scope.as_ref())
                        .map_err_as_cause(|| ModelErrorDetail::Expr)?;
                    for n in res.into_iter() {
                        e.apply_ext(self.root(), &n, scope.as_ref())
                            .map_err_as_cause(|| ModelErrorDetail::Expr)?;
                    }
                }
                scope.remove_func(LOAD_FILE_FUNC_NAME);
            }
        }
        Ok(())
    }

    /// Add `$defines`, `$hosts`, `$users`, `$procs` variables to model scope.
    /// Variables are computed from `Defines` defined in `Manifest`.
    fn set_defines(&mut self, manifest: &Manifest) {
        let defs = manifest.defines().to_node();
        if manifest.defines().is_user_defined() {
            defs.data_mut().set_file(Some(&FileInfo::new(
                &self.metadata.path().join(DEFAULT_MANIFEST_FILENAME),
                FileType::File,
                FileFormat::Toml,
            )));
        }

        let scope_def = self.scoped.scope_def_mut();

        scope_def.set_var_def("$defines".into(), ValueDef::Static(defs.into()));
        scope_def.set_var_def(
            "$hosts".into(),
            ValueDef::Resolvable(manifest.defines().hosts().clone()),
        );
        scope_def.set_var_def(
            "$users".into(),
            ValueDef::Resolvable(manifest.defines().users().clone()),
        );
        scope_def.set_var_def(
            "$procs".into(),
            ValueDef::Resolvable(manifest.defines().procs().clone()),
        );
    }

    pub fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    pub fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }

    pub fn set_metadata(&mut self, metadata: Metadata) {
        self.metadata = metadata;
    }

    pub fn hosts(&self) -> &[HostDef] {
        &self.hosts
    }

    pub fn users(&self) -> &[UserDef] {
        &self.users
    }

    pub fn procs(&self) -> &[ProcDef] {
        &self.procs
    }

    pub fn get_host(&self, node: &NodeRef) -> Option<&HostDef> {
        self.lookup.get(node)
    }

    pub fn get_host_path(&self, node_path: &Opath) -> Option<&HostDef> {
        self.lookup.get_path(self.root(), node_path)
    }

    pub fn get_user(&self, node: &NodeRef) -> Option<&UserDef> {
        self.lookup.get(node)
    }

    pub fn get_user_path(&self, node_path: &Opath) -> Option<&UserDef> {
        self.lookup.get_path(self.root(), node_path)
    }

    pub fn get_proc(&self, node: &NodeRef) -> Option<&ProcDef> {
        self.lookup.get(node)
    }

    pub fn get_proc_path(&self, node_path: &Opath) -> Option<&ProcDef> {
        self.lookup.get_path(self.root(), node_path)
    }

    pub fn get_task(&self, node: &NodeRef) -> Option<&TaskDef> {
        self.lookup.get(node)
    }

    pub fn get_task_path(&self, node_path: &Opath) -> Option<&TaskDef> {
        self.lookup.get_path(self.root(), node_path)
    }

    pub fn resolve_path<P1, P2>(&self, path: P1, current_dir: P2) -> PathBuf
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        resolve_model_path(path, current_dir, self.metadata.path())
    }

    unsafe fn init(&self) {
        unsafe fn init_proc<T: AsScoped>(lookup: &mut ModelLookup, parent: &T, p: &ProcDef) {
            lookup.put(p);
            parent.as_scoped().add_child(p);
            for s in p.run().steps() {
                for t in s.tasks() {
                    lookup.put(t);
                    p.as_scoped().add_child(t);
                    if let Some(switch) = t.switch() {
                        for c in switch.cases() {
                            init_proc(lookup, t, c.proc());
                        }
                    }
                }
            }
        }

        let mut lookup = ModelLookup::new();
        for h in self.hosts() {
            lookup.put(h);
        }
        for u in self.users() {
            lookup.put(u);
        }
        for p in self.procs() {
            init_proc(&mut lookup, self, p);
        }
        let ptr = std::mem::transmute::<&Model, *const ()>(self);
        let mut_model = std::mem::transmute::<*const (), &mut Model>(ptr);

        mut_model.lookup = lookup;
    }

    fn reset(&self) {
        self.as_scoped().clear_scope();
    }

    fn deep_copy(&self) -> Self {
        let mut node_path_map = HashMap::new();
        self.root().visit_recursive(|_, _, n| {
            node_path_map.insert(n.data_ptr(), n.path());
            true
        });

        let root = self.root().deep_copy();
        let mut path_node_map = HashMap::with_capacity(node_path_map.len());
        root.visit_recursive(|_, _, n| {
            path_node_map.insert(n.path(), n.clone());
            true
        });

        let mut node_map = NodeMap::with_capacity(node_path_map.len());
        for (n, p) in node_path_map {
            let nn = path_node_map[&p].clone();
            node_map.insert(n, nn);
        }

        std::mem::drop(path_node_map);

        let mut m = Model {
            scoped: Scoped::new(self.root(), self.root(), self.scoped.scope_def().clone()),
            metadata: self.metadata.clone(),
            hosts: self.hosts.clone(),
            users: self.users.clone(),
            procs: self.procs.clone(),
            lookup: ModelLookup::new(),
            logger: self.logger.clone(),
        };

        m.remap(&node_map);

        m
    }
}

impl AsScoped for Model {
    fn as_scoped(&self) -> &Scoped {
        &self.scoped
    }
}

impl Remappable for Model {
    fn remap(&mut self, node_map: &NodeMap) {
        self.scoped.remap(node_map);
        self.hosts.iter_mut().for_each(|h| h.remap(node_map));
        self.users.iter_mut().for_each(|u| u.remap(node_map));
        self.procs.iter_mut().for_each(|p| p.remap(node_map));
    }
}

impl ModelDef for Model {
    fn root(&self) -> &NodeRef {
        self.as_scoped().root()
    }

    fn node(&self) -> &NodeRef {
        self.as_scoped().node()
    }
}

impl ScopedModelDef for Model {
    fn scope_def(&self) -> &ScopeDef {
        self.as_scoped().scope_def()
    }

    fn scope(&self) -> DefsResult<&Scope> {
        self.as_scoped().scope()
    }

    fn scope_mut(&self) -> DefsResult<&ScopeMut> {
        self.as_scoped().scope_mut()
    }
}

struct ModelLookup {
    node_map: HashMap<*const Node, &'static dyn ModelDef>,
    path_map: HashMap<Opath, &'static dyn ModelDef>,
}

impl ModelLookup {
    fn new() -> ModelLookup {
        ModelLookup {
            node_map: HashMap::new(),
            path_map: HashMap::new(),
        }
    }

    fn get<T: ModelDef>(&self, node: &NodeRef) -> Option<&T> {
        if let Some(&p) = self.node_map.get(&node.data_ptr()) {
            p.downcast_ref::<T>()
        } else {
            None
        }
    }

    fn get_path<T: ModelDef>(&self, _root: &NodeRef, node_path: &Opath) -> Option<&T> {
        if let Some(&p) = self.path_map.get(node_path) {
            p.downcast_ref::<T>()
        } else {
            None
        }
    }

    fn put(&mut self, def: &dyn ModelDef) {
        self.node_map.insert(def.node().data_ptr(), unsafe {
            std::mem::transmute::<&dyn ModelDef, &'static dyn ModelDef>(def)
        });
        self.path_map.insert(def.node().path(), unsafe {
            std::mem::transmute::<&dyn ModelDef, &'static dyn ModelDef>(def)
        });
    }
}

impl std::fmt::Debug for ModelLookup {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use std::raw::TraitObject;

        struct ModelDefDebug(&'static dyn ModelDef);

        impl std::fmt::Debug for ModelDefDebug {
            fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                let type_id = self.0.type_id();
                let ptr = unsafe { std::mem::transmute::<&dyn ModelDef, TraitObject>(self.0).data };
                if type_id == TypeId::of::<Model>() {
                    write!(f, "Model<{:p}>", ptr)
                } else if type_id == TypeId::of::<HostDef>() {
                    write!(f, "HostDef<{:p}>", ptr)
                } else if type_id == TypeId::of::<UserDef>() {
                    write!(f, "UserDef<{:p}>", ptr)
                } else if type_id == TypeId::of::<ProcDef>() {
                    write!(f, "ProcedureDef<{:p}>", ptr)
                } else if type_id == TypeId::of::<TaskDef>() {
                    write!(f, "TaskDef<{:p}>", ptr)
                } else {
                    unreachable!();
                }
            }
        }

        let mut s = f.debug_map();
        for (k, v) in self.path_map.iter() {
            s.entry(&k.to_string(), &ModelDefDebug(*v));
        }
        s.finish()
    }
}

#[derive(Debug, Clone)]
pub struct ModelRef(Arc<ReentrantMutex<Model>>);

impl ModelRef {
    fn new(model: Model) -> ModelRef {
        let m = ModelRef(Arc::new(ReentrantMutex::new(model)));
        unsafe { m.lock().init() };
        m
    }

    /// Read model for provided metadata.
    /// Returns error if `metadata.path()` is not model dir
    pub fn read(metadata: Metadata, logger: Logger) -> ModelResult<ModelRef> {
        Ok(Self::new(Model::read_revision(metadata, logger)?))
    }

    pub fn lock(&self) -> ReentrantMutexGuard<Model> {
        self.0.lock()
    }

    pub fn deep_copy(&self) -> ModelRef {
        let m = self.lock();
        Self::new(m.deep_copy())
    }

    pub fn reset(&self) {
        self.lock().reset();
    }

    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }
}

impl Default for ModelRef {
    fn default() -> Self {
        ModelRef(Arc::new(ReentrantMutex::new(Model::empty())))
    }
}

impl PartialEq for ModelRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for ModelRef {}

unsafe impl Send for ModelRef {}

unsafe impl Sync for ModelRef {}

/// Resolve path defined in model.
/// # Arguments
///
/// * `path` - path to resolve
/// * `current_dir` - directory to resolve relative paths
/// * `model_dir` - absolute path to model directory
///
/// # Returns
/// Absolute path
///
/// # Example
/// ```
/// use std::path::PathBuf;
/// use op_model::resolve_model_path;
///
/// // path relative to current dir
/// let p = resolve_model_path("./some_file.yaml", "current_dir", "/abs/path/model_dir");
/// assert_eq!(PathBuf::from("/abs/path/model_dir/current_dir/some_file.yaml"), p);
///
/// // path relative to model_dir
/// let p = resolve_model_path("some_file.yaml", "current_dir", "/abs/path/model_dir");
/// assert_eq!(PathBuf::from("/abs/path/model_dir/some_file.yaml"), p);
///
/// // absolute path
/// let p = resolve_model_path("/some/abs/path/some_file.yaml", "current_dir", "/abs/path/model_dir");
/// assert_eq!(PathBuf::from("/some/abs/path/some_file.yaml"), p);
/// ```
pub fn resolve_model_path<P1, P2, P3>(path: P1, current_dir: P2, model_dir: P3) -> PathBuf
where
    P1: AsRef<Path>,
    P2: AsRef<Path>,
    P3: AsRef<Path>,
{
    const PREFIX: &str = "./";

    if path.as_ref().is_absolute() {
        path.as_ref().to_owned()
    } else if path.as_ref().starts_with(PREFIX) {
        let path = path.as_ref().strip_prefix(PREFIX).unwrap();
        if current_dir.as_ref().is_absolute() {
            current_dir.as_ref().join(path)
        } else {
            model_dir.as_ref().join(current_dir.as_ref()).join(path)
        }
    } else {
        model_dir.as_ref().join(path.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod resolve_model_path {
        use super::*;
        #[test]
        fn relative_to_current() {
            let p = resolve_model_path("./dir/some_file.yaml", "current_dir", "/abs/model_dir");
            assert_eq!(
                PathBuf::from("/abs/model_dir/current_dir/dir/some_file.yaml"),
                p
            );

            let p = resolve_model_path("./some_file.yaml", "/abs/current_dir", "/model_dir");
            assert_eq!(PathBuf::from("/abs/current_dir/some_file.yaml"), p);

            let p = resolve_model_path("./../some_file.yaml", "/abs/current_dir", "/model_dir");
            assert_eq!(PathBuf::from("/abs/current_dir/../some_file.yaml"), p);
        }

        #[test]
        fn relative_to_model() {
            let p = resolve_model_path("dir/some_file.yaml", "whatever", "/abs/model_dir");
            assert_eq!(PathBuf::from("/abs/model_dir/dir/some_file.yaml"), p);

            let p = resolve_model_path("some_file.yaml", "whatever", "/abs/model_dir");
            assert_eq!(PathBuf::from("/abs/model_dir/some_file.yaml"), p);

            let p = resolve_model_path("../some_file.yaml", "whatever", "/abs/model_dir");
            assert_eq!(PathBuf::from("/abs/model_dir/../some_file.yaml"), p);

            let p = resolve_model_path("dir/../some_file.yaml", "whatever", "/abs/model_dir");
            assert_eq!(PathBuf::from("/abs/model_dir/dir/../some_file.yaml"), p);
        }

        #[test]
        fn absolute() {
            let p = resolve_model_path("/abs/path/some_file.yaml", "whatever", "whatever");
            assert_eq!(PathBuf::from("/abs/path/some_file.yaml"), p);
        }
    }
}
