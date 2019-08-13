use std::any::TypeId;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;

use git2::{ObjectType, TreeWalkMode, TreeWalkResult};

use parking_lot::{ReentrantMutex, ReentrantMutexGuard};

use super::*;
use kg_diag::{BasicDiag, Diag, Severity};

pub type ModelError = BasicDiag;
pub type ModelResult<T> = Result<T, ModelError>;

#[derive(Debug, Display, Detail)]
#[diag(code_offset = 1000)]
pub enum ModelErrorDetail {
    #[display(fmt = "cannot parse config file: '{file_path}' : {err}")]
    MalformedConfigFile {
        file_path: String,
        err: Box<dyn Diag>,
    },

    #[display(fmt = "cannot get config, git error occurred : {err}")]
    ConfigGitErr { err: Box<dyn Diag> },

    #[display(fmt = "cannot parse manifest file '{file_path}': {err}")]
    MalformedManifest {
        file_path: String,
        err: Box<dyn Diag>,
    },

    #[display(fmt = "cannot find manifest file")]
    ManifestNotFount,

    #[display(fmt = "Config file '{file_path}' is not valid utf-8")]
    ConfigUtf8Err { file_path: String },

    #[display(
        fmt = "cannot resolve interpolations: {detail}",
        detail = "err.detail()"
    )]
    InterpolationsResolveErr { err: Box<dyn Diag> },

    #[display(fmt = "cannot evaluate expression: {detail}", detail = "err.detail()")]
    ExprErr { err: Box<dyn Diag> },

    #[display(fmt = "cannot generate model diff: {detail}", detail = "err.detail()")]
    ModelDiffErr { err: Box<dyn Diag> },
}

impl ModelErrorDetail {
    pub fn config_git_err(err: BasicDiag) -> ModelError {
        ModelErrorDetail::ConfigGitErr { err: Box::new(err) }.into()
    }
}

#[derive(Debug, Clone)]
struct LoadFileFunc {
    model_dir: PathBuf,
    model_oid: Sha1Hash,
}

impl LoadFileFunc {
    fn new(model_dir: PathBuf, model_oid: Sha1Hash) -> Self {
        Self {
            model_dir,
            model_oid,
        }
    }
}

impl FuncCallable for LoadFileFunc {
    fn call(&self, name: &str, args: Args, env: Env, out: &mut NodeBuf) -> FuncCallResult {
        args.check_count_func(&FuncId::Custom(name.to_string()), 1, 2)?;

        let git = GitManager::new(&self.model_dir)?;
        let tree = git.get_tree(&self.model_oid.into())?;
        let odb = git.odb()?;

        let paths = args.resolve_column(false, 0, env).map_err(|d| {
            FuncCallErrorDetail::FuncCallCustomErr {
                id: FuncId::Custom(name.to_string()),
                err: Box::new(d),
            }
        })?;

        if args.count() == 1 {
            for path in paths.into_iter() {
                let path = PathBuf::from(path.as_string());
                let entry = tree
                    .get_path(&path)
                    .map_err(|err| GitErrorDetail::GetFile {
                        file: path.clone(),
                        err,
                    })?;
                let obj = odb
                    .read(entry.id())
                    .map_err(|err| GitErrorDetail::GetFile {
                        file: path.clone(),
                        err,
                    })?;

                let format = path.extension().map_or(FileFormat::Text, |ext| {
                    FileFormat::from(ext.to_str().unwrap())
                });

                let node = NodeRef::from_bytes(obj.data(), format)?;
                out.add(node)
            }
        } else {
            let formats = args.resolve_column(false, 1, env).map_err(|d| {
                FuncCallErrorDetail::FuncCallCustomErr {
                    id: FuncId::Custom(name.to_string()),
                    err: Box::new(d),
                }
            })?;

            for (p, f) in paths.into_iter().zip(formats.into_iter()) {
                let path = PathBuf::from(p.as_string());
                let entry = tree
                    .get_path(&path)
                    .map_err(|err| GitErrorDetail::GetFile {
                        file: path.clone(),
                        err,
                    })?;
                let obj = odb
                    .read(entry.id())
                    .map_err(|err| GitErrorDetail::GetFile {
                        file: path.clone(),
                        err,
                    })?;

                let format: FileFormat = f.data().as_string().as_ref().into();

                let node = NodeRef::from_bytes(obj.data(), format)?;
                out.add(node)
            }
        }

        Ok(())
    }

    fn clone(&self) -> Box<dyn FuncCallable> {
        Box::new(std::clone::Clone::clone(self))
    }
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
        }
    }

    pub fn load_manifest(model_dir: &Path) -> ModelResult<Manifest> {
        let path = model_dir.join(PathBuf::from(DEFAULT_MANIFEST_FILENAME));
        let mut content = String::new();
        fs::read_to_string(&path, &mut content)?;
        let manifest: Manifest = kg_tree::serial::toml::from_str(&content).map_err(|err| {
            ModelErrorDetail::MalformedManifest {
                err: Box::new(err),
                file_path: path.to_string_lossy().to_string(),
            }
        })?;
        Ok(manifest)
    }

    pub fn read_revision(metadata: Metadata) -> ModelResult<Model> {
        let manifest = Model::load_manifest(metadata.path())?;

        kg_tree::set_base_path(metadata.path());

        let mut m = Model {
            metadata,
            ..Model::empty()
        };

        let cr = ConfigResolver::scan_revision(m.metadata.path(), &m.metadata.id())?;

        m.root().data_mut().set_file(Some(&FileInfo::new(
            m.metadata.path(),
            FileType::Dir,
            FileFormat::Binary,
        )));

        let scope = ScopeMut::new();

        m.resolve_includes(&cr, &scope)?;
        m.set_defines(&manifest);
        m.resolve_overrides(&cr, &scope)?;
        m.resolve_interpolations()?;

        return Ok(m);
    }

    /// Walk through each entry in model directory, resolve matching `Includes` and apply changes to model tree
    fn resolve_includes(&mut self, cr: &ConfigResolver, scope: &ScopeMut) -> ModelResult<()> {
        let model = self;
        let commit = model.metadata.id();
        let model_dir = model.metadata.path().to_owned();

        scope.set_func(
            "loadFile".into(),
            Box::new(LoadFileFunc::new(model_dir.clone(), commit)),
        );

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
                    eprintln!("Unknown git object type, skipping = {:?}", entry.kind());
                    return TreeWalkResult::Ok;
                }
            };

            if file_type == FileType::File
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

                    let item = inc
                        .item()
                        .apply_one_ext(model.root(), &n, scope.as_ref())
                        .map_err(|err| ModelErrorDetail::ExprErr { err: Box::new(err) })?;

                    if item.data().file().is_none() {
                        item.data_mut().set_file(Some(&file_info));
                    }

                    scope.set_var("item".into(), NodeSet::One(item));

                    inc.mapping()
                        .apply_ext(model.root(), model.root(), scope.as_ref())
                        .map_err(|err| ModelErrorDetail::ExprErr { err: Box::new(err) })?;
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

    /// Resolve each interpolation and apply changes to model tree
    fn resolve_interpolations(&mut self) -> ModelResult<()> {
        let model = self;
        let mut resolver = TreeResolver::new();
        resolver.resolve(model.root()).map_err(|err| {
            BasicDiag::from(ModelErrorDetail::InterpolationsResolveErr { err: Box::new(err) })
        })?;
        {
            let scope = model.scoped.scope_mut()?;

            // definitions (hosts, users, processors)
            //FIXME (jc) error handling
            let mut hosts = Vec::new();
            for h in scope.get_var("$hosts").unwrap().iter() {
                match HostDef::parse(&model, &model.scoped, h) {
                    Ok(host) => hosts.push(host),
                    Err(_err) => eprintln!("err"),
                }
            }
            model.hosts = hosts;

            let mut users = Vec::new();
            for u in scope.get_var("$users").unwrap().iter() {
                match UserDef::parse(&model, &model.scoped, u) {
                    Ok(user) => users.push(user),
                    Err(_err) => eprintln!("err"),
                }
            }
            model.users = users;

            let mut procs = Vec::new();
            for p in scope.get_var("$procs").unwrap().iter() {
                match ProcDef::parse(&model, &model.scoped, p) {
                    Ok(p) => procs.push(p),
                    Err(_err) => eprintln!("err"),
                }
            }
            model.procs = procs;
        }
        Ok(())
    }

    /// Resolve each `override` an apply changes to model tree.
    fn resolve_overrides(&mut self, cr: &ConfigResolver, scope: &ScopeMut) -> ModelResult<()>{
        let model = self;
        for (path, config) in cr.iter() {
            if !config.overrides().is_empty() {
                let path = if path.as_os_str().is_empty() {
                    Opath::parse("@").unwrap()
                } else {
                    Opath::parse(&path.to_str().unwrap().replace('/', ".")).unwrap()
                };

                let current = path
                    .apply_one_ext(model.root(), model.root(), scope.as_ref())
                    .map_err(|err| ModelErrorDetail::ExprErr { err: Box::new(err) })?;

                for (p, e) in config.overrides().iter() {
                    let res = p
                        .apply_ext(model.root(), &current, scope.as_ref())
                        .map_err(|err| ModelErrorDetail::ExprErr { err: Box::new(err) })?;
                    for n in res.into_iter() {
                        e.apply_ext(model.root(), &n, scope.as_ref())
                            .map_err(|err| ModelErrorDetail::ExprErr { err: Box::new(err) })?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Add `$defines`, `$hosts`, `$users`, `$procs` variables to model scope.
    /// Variables are computed from `Defines` defined in `Manifest`.
    fn set_defines(&mut self, manifest: &Manifest) {
        let model = self;
        let defs = manifest.defines().to_node();
        if manifest.defines().is_user_defined() {
            defs.data_mut().set_file(Some(&FileInfo::new(
                &model.metadata.path().join(DEFAULT_MANIFEST_FILENAME),
                FileType::File,
                FileFormat::Toml,
            )));
        }

        let scope_def = model.scoped.scope_def_mut();

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
    pub fn read(metadata: Metadata) -> ModelResult<ModelRef> {
        Ok(Self::new(Model::read_revision(metadata)?))
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

/*#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn read_test() {
        let mut metadata = Metadata::default();
        metadata.set_id(Sha1Hash::from_str("e2ed3a7c0d98592fec674d60c7176db66ef7e09b").unwrap());
        metadata
            .set_path(PathBuf::from_str("/home/wiktor/Desktop/opereon/resources/model/").unwrap());
        let model = Model::read_revision(metadata).expect("Cannot read model");
        eprintln!("model = {}", serde_json::to_string_pretty(&model).unwrap());
    }
}*/
