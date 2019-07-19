use std::sync::{Arc, Mutex, MutexGuard};

use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "arg")]
pub enum Outcome {
    Empty,
    NodeSet(NodeSetRef),
    Diff(ModelDiff),
    File(PathBuf),
    Many(Vec<Outcome>),
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            Outcome::Empty => write!(f, "<empty>"),
            Outcome::NodeSet(ref s) => write!(f, "<data> {}", s.lock()),
            Outcome::Diff(ref d) => write!(f, "<diff> {}", d),
            Outcome::File(ref p) => write!(f, "<file> {}", p.display()),
            Outcome::Many(ref elems) => {
                write!(f, "<many> {{ ")?;
                let mut it = elems.iter().peekable();
                while let Some(e) = it.next() {
                    if it.peek().is_some() {
                        write!(f, "{}, ", e)?;
                    } else {
                        write!(f, "{}", e)?;
                    }
                }
                write!(f, " }}")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct NodeSetRef(Arc<Mutex<NodeSet>>);

impl NodeSetRef {
    pub fn lock(&self) -> MutexGuard<NodeSet> {
        self.0.lock().unwrap()
    }
}

impl From<NodeSet> for NodeSetRef {
    fn from(ns: NodeSet) -> Self {
        lazy_static! {
            static ref LOCK: Mutex<()> = Mutex::new(());
        }
        let _lock = LOCK.lock().unwrap();
        NodeSetRef(Arc::new(Mutex::new(ns.into_consumable())))
    }
}

impl From<NodeRef> for NodeSetRef {
    fn from(nr: NodeRef) -> Self {
        Self::from(NodeSet::from(nr))
    }
}

impl PartialEq for NodeSetRef {
    fn eq(&self, other: &NodeSetRef) -> bool {
        if Arc::ptr_eq(&self.0, &other.0) {
            true
        } else {
            self.lock().deref() == other.lock().deref()
        }
    }
}

impl ser::Serialize for NodeSetRef {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: ser::Serializer,
    {
        self.lock().serialize(serializer)
    }
}

impl<'de> de::Deserialize<'de> for NodeSetRef {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        let n = NodeSet::deserialize(deserializer)?;
        Ok(n.into())
    }
}

unsafe impl Send for NodeSetRef {}

unsafe impl Sync for NodeSetRef {}
