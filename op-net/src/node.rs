use super::*;

use std::hash::{Hash, Hasher};

use chrono::prelude::*;
use hostname;
use std;
use std::net::SocketAddrV4;
use std::path::Path;
use toml;
use uuid::Uuid;
use ifaces;
use core::fmt;

#[derive(PartialEq, Eq, Deserialize, Serialize, Clone)]
pub struct NodeInfo {
    pub id: Uuid,
    pub name: Option<String>,
    pub ipv4: Vec<SocketAddrV4>,
    pub time: DateTime<Utc>,
}

impl Hash for NodeInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write(self.id.as_bytes())
    }
}

impl NodeInfo {
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    pub fn read<P: AsRef<Path>>(path: P) -> IoResult<NodeInfo> {
        let mut c = String::new();
        kg_io::fs::read_to_string(path, &mut c)?;
        Ok(toml::from_str(&c).unwrap()) //FIXME (jc) handle errors
    }

    pub fn write<P: AsRef<Path>>(&self, path: P) -> IoResult<()> {
        let c = toml::to_string_pretty(self).unwrap();
        kg_io::fs::write(path, c)
    }


    pub fn generate() -> NodeInfo {
        NodeInfo {
            id: Uuid::new_v4(),
            name : hostname::get_hostname(),
            time: Utc::now(),
            ipv4: ifaces::get_up_ipv4().expect("Cannot get up interfaces")
        }
    }

    pub fn is_empty(&self) -> bool {
        self.id.is_nil()
    }

    pub fn empty() -> NodeInfo {
        NodeInfo {
            id: Uuid::nil(),
            name: None,
            ipv4: vec![],
            time: Utc::now(),
        }
    }
}
impl std::fmt::Display for NodeInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", toml::to_string_pretty(self).unwrap())
    }
}

impl fmt::Debug for NodeInfo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("NodeInfo")
            .field("id", &self.id.to_string())
            .field("name", &self.name)
            .field("ipv4", &self.ipv4)
            .field("time", &self.time)
            .finish()
    }
}

impl Default for NodeInfo {
    fn default() -> Self {
        NodeInfo::generate()
    }
}
