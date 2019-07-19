use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Host {
    hostname: String,
    ssh_dest: SshDest,
}

impl Host {
    pub fn from_def(host_def: &HostDef) -> Result<Host, ProtoError> {
        let mut h: Host = from_tree(host_def.node())?;
        h.hostname = host_def.hostname().to_string();
        if h.ssh_dest.hostname().is_empty() {
            h.ssh_dest.set_hostname(&h.hostname);
        }
        if h.ssh_dest.username().is_empty() {
            h.ssh_dest.set_username_current();
        }
        Ok(h)
    }

    pub fn from_dest(ssh_dest: SshDest) -> Host {
        Host {
            hostname: ssh_dest.hostname().to_string(),
            ssh_dest,
        }
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn ssh_dest(&self) -> &SshDest {
        &self.ssh_dest
    }
}

impl std::fmt::Display for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.hostname)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn as_host() -> Host {
        Host {
            hostname: "h1.kodegenix.pl".into(),
            ssh_dest: SshDest::new(
                "h1.kodegenix.pl",
                22,
                "root",
                SshAuth::PublicKey {
                    identity_file: PathBuf::from("~/.ssh/id_rsa"),
                },
            ),
        }
    }

    fn as_json() -> &'static str {
        r#"{
          "hostname": "h1.kodegenix.pl",
          "ssh_dest": {
            "hostname": "h1.kodegenix.pl",
            "port": 22,
            "username": "root",
            "auth": {
              "method": "public-key",
              "identity_file": "~/.ssh/id_rsa"
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
}
