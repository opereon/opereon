use super::*;

use std::any::TypeId;
use std::cell::{Cell, RefCell};


//FIXME (jc) collect error kinds, implement DiagsKind
#[derive(Debug)]
pub enum DefsParseError {
    Undef,
}

//FIXME (jc)
impl From<opath::OpathParseError> for DefsParseError {
    fn from(err: opath::OpathParseError) -> Self {
        println!("from: {:?}", err);
        DefsParseError::Undef
    }
}

//FIXME (jc) to be removed
macro_rules! perr {
    ( $msg:expr ) => {{
        eprintln!("ERROR in {}:{} - {}", file!(), line!(), $msg);
        Err(DefsParseError::Undef)
    }}
}

//FIXME (jc) to be removed
macro_rules! perr_assert {
    ( $cond:expr, $msg:expr ) => {{
        if $cond {
            Ok(())
        } else {
            perr!($msg)
        }
    }}
}


mod host;
mod user;
mod proc;
mod scope;

pub use self::host::HostDef;
pub use self::user::UserDef;
pub use self::proc::*;
pub use self::scope::*;


pub trait ModelDef: Remappable + 'static {
    fn root(&self) -> &NodeRef;

    fn node(&self) -> &NodeRef;

    #[inline]
    fn type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
}

impl ModelDef {
    pub(super) fn downcast_ref<T: ModelDef>(&self) -> Option<&T> {
        if self.type_id() == TypeId::of::<T>() {
            unsafe { Some(&*(self as *const ModelDef as *const T)) }
        } else {
            None
        }
    }
}

pub trait ScopedModelDef: ModelDef {
    fn scope_def(&self) -> &ScopeDef;

    fn scope(&self) -> &Scope;

    fn scope_mut(&self) -> &ScopeMut;
}

pub (super) trait ParsedModelDef: Sized {
    fn parse(model: &Model, parent: &Scoped, node: &NodeRef) -> Result<Self, DefsParseError>;
}

pub(crate) trait AsScoped: 'static {
    fn as_scoped(&self) -> &Scoped;
}


fn get_expr<T: Primitive>(def: &ModelDef, expr: &str) -> T {
    let expr = Opath::parse(expr).unwrap();
    match expr.apply(def.root(), def.node()).into_one() {
        Some(n) => T::get(&n),
        None => T::empty(),
    }
}


#[derive(Debug, Serialize)]
pub(crate) struct Scoped {
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

    pub fn scope(&self) -> &Scope {
        self.resolve();
        &self.scope
    }

    pub fn scope_mut(&self) -> &ScopeMut {
        self.resolve();
        self.invalidate();
        &self.scope
    }

    pub unsafe fn add_child<T: AsScoped>(&self, child: &T) {
        child.as_scoped().parent.set(Some(std::mem::transmute::<&Scoped, &'static Scoped>(self)));
        child.as_scoped().scope.set_parent(Some(self.scope.clone().into()));
        child.as_scoped().resolved.set(false);
        self.children.borrow_mut().push(std::mem::transmute::<&Scoped, &'static Scoped>(child.as_scoped()));
    }

    pub fn clear_scope(&self) {
        self.scope.clear_vars();
        self.resolved.set(false);
        for s in self.children.borrow().iter().cloned() {
            s.clear_scope();
        }
    }

    fn resolve(&self) {
        if !self.resolved.get() {
            if let Some(p) = self.parent.get() {
                p.resolve();
            }
            self.scope_def.resolve(self.root(), self.node(), &self.scope);
            self.resolved.set(true);
        }
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

