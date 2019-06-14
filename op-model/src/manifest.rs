use super::*;

pub static DEFAULT_MANIFEST_FILENAME: &'static str = "op.toml";

#[inline(always)]
fn user_defined() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Defines {
    users: Opath,
    hosts: Opath,
    procs: Opath,
    #[serde(flatten)]
    custom: LinkedHashMap<String, Opath>,
    #[serde(skip, default = "user_defined")]
    user_defined: bool,
}

impl Defines {
    pub fn new() -> Self {
        Defines {
            users: Opath::parse("$.conf.users.*").unwrap(),
            hosts: Opath::parse("$.conf.hosts.*").unwrap(),
            procs: Opath::parse("$.(proc, probe).**[@.proc != null]").unwrap(),
            custom: LinkedHashMap::new(),
            user_defined: false,
        }
    }

    pub fn users(&self) -> &Opath {
        &self.users
    }

    pub fn hosts(&self) -> &Opath {
        &self.hosts
    }

    pub fn procs(&self) -> &Opath {
        &self.procs
    }

    pub fn custom(&self) -> &LinkedHashMap<String, Opath> {
        &self.custom
    }

    pub fn is_user_defined(&self) -> bool {
        self.user_defined
    }

    pub fn to_node<'a>(&self) -> NodeRef {
        let mut p = Properties::new();
        p.insert("users".into(), NodeRef::string(self.users().to_string()));
        p.insert("hosts".into(), NodeRef::string(self.hosts().to_string()));
        p.insert("procs".into(), NodeRef::string(self.procs().to_string()));
        for (name, expr) in self.custom().iter() {
            p.insert(name.into(), NodeRef::string(expr.to_string()));
        }
        NodeRef::object(p)
    }
}

impl Default for Defines {
    fn default() -> Self {
        Defines::new()
    }
}


#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestInfo {
    authors: Vec<String>,
    description: String,
}

impl Default for ManifestInfo {
    fn default() -> Self {
        ManifestInfo {
            authors: Vec::new(),
            description: String::new(),
        }
    }
}



#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Manifest {
    info: ManifestInfo,
    defines: Defines,
}

impl Default for Manifest {
    fn default() -> Self {
        Manifest {
            info: ManifestInfo::default(),
            defines: Defines::default(),
        }
    }
}

impl Manifest {
    pub fn defines(&self) -> &Defines {
        &self.defines
    }

    pub fn info(&self) -> &ManifestInfo {
        &self.info
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    mod defines {
        use super::*;

        #[test]
        fn serialize_with_custom_defs() {
            let mut d = Defines::default();
            d.custom.insert("cust1".into(), Opath::parse("$.cust1").unwrap());
            d.custom.insert("cust2".into(), Opath::parse("$.cust2").unwrap());

            let json = r#"
            {
              "users": "${$.conf.users.*}",
              "hosts": "${$.conf.hosts.*}",
              "procs": "${$.(proc, probe).**[(@.proc != null)]}",
              "cust1": "${$.cust1}",
              "cust2": "${$.cust2}"
            }
            "#;

            let s = serde_json::to_string_pretty(&d).unwrap();
//            assert!(json_eq!(json, &s));
        }

        #[test]
        fn deserialize_with_custom_defs() {
            let json = r#"
            {
              "users": "$.conf.users",
              "hosts": "$.conf.hosts",
              "procs": "$.proc.**[@.proc != null]",
              "cust1": "$.cust1",
              "cust2": "$.cust2"
            }
            "#;

            let d: Defines = serde_json::from_str(json).unwrap();
            assert_eq!(d.users, Opath::parse("$.conf.users").unwrap());
            assert_eq!(d.hosts, Opath::parse("$.conf.hosts").unwrap());
            assert_eq!(d.procs, Opath::parse("$.proc.**[@.proc != null]").unwrap());
            assert_eq!(d.custom.len(), 2);
            assert_eq!(d.custom["cust1"], Opath::parse("$.cust1").unwrap());
            assert_eq!(d.custom["cust2"], Opath::parse("$.cust2").unwrap());
            assert!(d.is_user_defined());
        }
    }
}
