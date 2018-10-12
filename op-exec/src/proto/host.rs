use super::*;


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    hostname: String,
    domain: String,
    ssh_dest: SshDest,
}

impl Host {
    pub fn from_def(host_def: &HostDef) -> Result<Host, ProtoError> {
        let h = from_tree(host_def.node())?;
        Ok(h)
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn domain(&self) -> &str {
        &self.domain
    }

    pub fn ssh_dest(&self) -> &SshDest {
        &self.ssh_dest
    }
}

impl std::fmt::Display for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.domain.is_empty() {
            write!(f, "{}", self.hostname)
        } else {
            write!(f, "{}.{}", self.hostname, self.domain)
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn as_host() -> Host {
        Host {
            hostname: "h1".into(),
            domain: "kodegenix.pl".into(),
            ssh_dest: SshDest::new(
                "h1.kodegenix.pl",
                22,
                "root",
                SshAuth::PublicKey { key_path: PathBuf::from("~/.ssh/id_rsa") }
            ),
        }
    }

    fn as_json() -> &'static str {
        r#"{
          "hostname": "h1",
          "domain": "kodegenix.pl",
          "ssh_dest": {
            "hostname": "h1.kodegenix.pl",
            "port": 22,
            "username": "root",
            "auth": {
              "method": "public-key",
              "key_path": "~/.ssh/id_rsa"
            }
          }
        }"#
    }

    #[test]
    fn can_deserialize_from_host_def() {
        let n = NodeRef::from_json(as_json()).unwrap();

        let host_def = HostDef::new(n.clone(), n.clone());
        let host = Host::from_def(&host_def).unwrap();

        assert_eq!(as_host(), host);
    }

    #[test]
    fn can_deserialize_from_json() {
        let host: Host = serde_json::from_str(as_json()).unwrap();

        assert_eq!(as_host(), host);
    }

    #[test]
    fn can_serialize() {
        let host = as_host();
        let json = serde_json::to_string(&host).unwrap();

        assert!(json_eq!(json, as_json()));
    }
}
