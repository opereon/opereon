use super::*;
use kg_diag::Severity;
use kg_diag::{BasicDiag, Diag};
use std::any::TypeId;
use std::cell::{Cell, RefCell};

pub use self::host::HostDef;
pub use self::proc::*;
pub use self::scope::*;
pub use self::user::UserDef;

pub type DefsError = BasicDiag;
pub type DefsResult<T> = Result<T, DefsError>;

//FIXME (jc) collect error kinds, implement Diags Kind
#[derive(Debug, Display, Detail)]
#[diag(code_offset = 900)]
pub enum DefsErrorDetail {
    #[display(fmt = "host definition must contain 'hostname' property")]
    HostMissingHostname,

    #[display(fmt = "host definition must contain 'ssh_dest' property")]
    HostMissingSshDest,

    #[display(fmt = "host definition must be an object, found: {kind}")]
    HostNonObject { kind: Kind },

    #[display(fmt = "procedure must have defined 'proc' property")]
    ProcMissingProc,

    #[display(fmt = "procedure definition must be an object, found: {kind}")]
    ProcNonObject { kind: Kind },

    #[display(fmt = "watch definition must be an object, found: {kind}")]
    ProcWatchNonObject { kind: Kind },

    #[display(fmt = "cannot parse model watch : {err}")]
    ProcModelWatchParseErr { err: Box<dyn Diag> },

    #[display(fmt = "cannot parse file watch : {err}")]
    ProcFileWatchParseErr { err: globset::Error },

    #[display(fmt = "run definition must be an object or an array, found: {kind}")]
    RunInvalidType { kind: Kind },

    #[display(fmt = "'hosts' property must be a dynamic expression in step definition")]
    StepStaticHosts,

    #[display(fmt = "invalid type of 'tasks' property in step definition")]
    StepInvalidTasksType { kind: Kind },

    #[display(fmt = "step definition must have 'tasks' property")]
    StepMissingTasks,

    #[display(fmt = "step definition must be an object, found: {kind}")]
    StepNonObject { kind: Kind },

    #[display(fmt = "task definition must have 'task' property")]
    TaskMissingTask,

    #[display(fmt = "switch task definition must have 'cases' property")]
    TaskSwitchMissingCases,

    #[display(fmt = "task definition must be an object, found: {kind}")]
    TaskNonObject { kind: Kind },

    #[display(fmt = "output definition must be an 'object' or 'string', found: {kind}")]
    TaskOutputInvalidType { kind: Kind },

    #[display(fmt = "Unexpected property type: {kind}")]
    TaskEnvUnexpectedPropType { kind: Kind },

    #[display(fmt = "switch definition must be an array, found: {kind}")]
    TaskSwitchNonArray { kind: Kind },

    #[display(fmt = "'when' property must be a dynamic expression in switch case definition")]
    TaskCaseStaticWhen,

    #[display(fmt = "switch case expression must have 'when' property")]
    TaskCaseMissingWhen,

    #[display(fmt = "switch case definition must be an object, found: {kind}")]
    TaskCaseNonObject { kind: Kind },

    #[display(fmt = "scope definition must be an object, found: '{kind}'")]
    ScopeNonObject { kind: Kind },

    #[display(fmt = "cannot get scope key '{key}' : {err}")]
    ScopeValParseErr { key: String, err: Box<dyn Diag> },

    #[display(fmt = "user definition must have 'username' property")]
    UserMissingUsername,

    #[display(fmt = "user definition must be an object, found: '{kind}'")]
    UserNonObject { kind: Kind },

    #[display(fmt = "unknown proc kind: '{value}'")]
    UnknownProcKind { value: String },

    #[display(fmt = "unknown task kind: '{value}'")]
    UnknownTaskKind { value: String },

    #[display(fmt = "cannot parse opath expression: {err}")]
    OpathParseErr { err: Box<dyn Diag> },

    #[display(fmt = "cannot evaluate expression: {err}")]
    ExprErr { err: Box<dyn Diag> },

    /// FIXME ws this variant should be replaced with Diag
    #[display(fmt = "serialization/deserialization error: {err}")]
    SerialErr { err: kg_tree::serial::Error },

    //FIXME ws to be removed
    #[display(fmt = "Error in line '{a0}'")]
    Undef(u32),
}

mod host;
mod proc;
mod scope;
mod user;

pub trait ModelDef: Remappable + 'static {
    fn root(&self) -> &NodeRef;

    fn node(&self) -> &NodeRef;

    #[inline]
    fn type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

impl dyn ModelDef {
    pub(super) fn downcast_ref<T: ModelDef>(&self) -> Option<&T> {
        if self.type_id() == TypeId::of::<T>() {
            unsafe { Some(&*(self as *const dyn ModelDef as *const T)) }
        } else {
            None
        }
    }
}

pub trait ScopedModelDef: ModelDef {
    fn scope_def(&self) -> &ScopeDef;

    fn scope(&self) -> DefsResult<&Scope>;

    fn scope_mut(&self) -> DefsResult<&ScopeMut>;
}

pub trait ParsedModelDef: Sized {
    fn parse(model: &Model, parent: &Scoped, node: &NodeRef) -> DefsResult<Self>;
}

pub trait AsScoped: 'static {
    fn as_scoped(&self) -> &Scoped;
}

fn get_expr<T: Primitive>(def: &dyn ModelDef, expr: &str) -> DefsResult<T> {
    let expr = Opath::parse(expr).unwrap();
    let res = expr
        .apply(def.root(), def.node())
        .map_err(|err| DefsErrorDetail::ExprErr { err: Box::new(err) })?;
    match res.into_one() {
        Some(n) => Ok(T::get(&n)),
        None => Ok(T::empty()),
    }
}

#[derive(Debug, Serialize)]
pub struct Scoped {
    #[serde(skip)]
    root: NodeRef,
    #[serde(skip)]
    node: NodeRef,
    scope_def: ScopeDef,
    #[serde(skip)]
    scope: ScopeMut,
    #[serde(skip)]
    parent: Cell<Option<&'static Scoped>>,
    #[serde(skip)]
    children: RefCell<Vec<&'static Scoped>>,
    #[serde(skip)]
    resolved: Cell<bool>,
}

impl Scoped {
    pub fn new(root: &NodeRef, node: &NodeRef, scope_def: ScopeDef) -> Scoped {
        Scoped {
            root: root.clone(),
            node: node.clone(),
            scope_def,
            scope: ScopeMut::new(),
            parent: Cell::new(None),
            children: RefCell::new(Vec::new()),
            resolved: Cell::new(false),
        }
    }

    pub fn root(&self) -> &NodeRef {
        &self.root
    }

    pub fn node(&self) -> &NodeRef {
        &self.node
    }

    pub fn scope_def(&self) -> &ScopeDef {
        &self.scope_def
    }

    pub fn scope_def_mut(&mut self) -> &mut ScopeDef {
        &mut self.scope_def
    }

    pub fn scope(&self) -> DefsResult<&Scope> {
        self.resolve()?;
        Ok(&self.scope)
    }

    pub fn scope_mut(&self) -> DefsResult<&ScopeMut> {
        self.resolve()?;
        self.invalidate();
        Ok(&self.scope)
    }

    pub unsafe fn add_child<T: AsScoped>(&self, child: &T) {
        child
            .as_scoped()
            .parent
            .set(Some(std::mem::transmute::<&Scoped, &'static Scoped>(self)));
        child
            .as_scoped()
            .scope
            .set_parent(Some(self.scope.clone().into()));
        child.as_scoped().resolved.set(false);
        self.children
            .borrow_mut()
            .push(std::mem::transmute::<&Scoped, &'static Scoped>(
                child.as_scoped(),
            ));
    }

    pub fn clear_scope(&self) {
        self.scope.clear_vars();
        self.resolved.set(false);
        for s in self.children.borrow().iter().cloned() {
            s.clear_scope();
        }
    }

    fn resolve(&self) -> DefsResult<()> {
        if !self.resolved.get() {
            if let Some(p) = self.parent.get() {
                p.resolve()?;
            }
            self.scope_def
                .resolve(self.root(), self.node(), &self.scope)?;
            self.resolved.set(true);
        }
        Ok(())
    }

    fn invalidate(&self) {
        self.resolved.set(false);
        for s in self.children.borrow().iter().cloned() {
            s.invalidate();
        }
    }
}

impl Remappable for Scoped {
    fn remap(&mut self, node_map: &NodeMap) {
        self.root = node_map.get(&self.root.data_ptr()).unwrap().clone();
        self.node = node_map.get(&self.node.data_ptr()).unwrap().clone();
        self.scope_def.remap(node_map);
        self.scope.clear_vars();
        self.resolved.set(false);
    }
}

impl Clone for Scoped {
    fn clone(&self) -> Scoped {
        let s = Scoped::new(self.root(), self.node(), self.scope_def.clone());
        for n in self.scope.func_names() {
            let f = self.scope.get_func(&n).unwrap().clone();
            s.scope.set_func(n, f);
        }
        for n in self.scope.method_names() {
            let m = self.scope.get_method(&n).unwrap().clone();
            s.scope.set_method(n, m);
        }
        s
    }
}
