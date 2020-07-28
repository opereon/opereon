use super::*;
use std::fmt::Debug;
use serde::export::Formatter;


#[derive(Clone, Serialize, Deserialize)]
pub struct RevInfo {
    /// Model identifier as git Oid
    id: Oid,
    /// Path to model dir
    path: PathBuf,
}

impl Debug for RevInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RevInfo")
            .field("id", &self.id().to_string())
            .field("path", &self.path)
            .finish()
    }
}

impl RevInfo {
    pub fn new(id: Oid, path: PathBuf) -> RevInfo {
        RevInfo {
            id,
            path,
        }
    }

    pub fn id(&self) -> Oid {
        self.id
    }

    pub fn set_id(&mut self, id: Oid) {
        self.id = id;
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.path = path;
    }
}

impl Default for RevInfo {
    fn default() -> Self {
        RevInfo {
            id: Oid::nil(),
            path: PathBuf::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_data() -> RevInfo {
        RevInfo::new(
            Oid::nil(),
            PathBuf::from("/home/example"),
        )
    }

    #[test]
    fn serialize_json() {
        let m = example_data();
        let res = serde_json::to_string(&m);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), r#"{"id":"0000000000000000000000000000000000000000","path":"/home/example"}"#)
    }

    #[test]
    fn serialize_yaml() {
        let m = example_data();
        let res = serde_yaml::to_string(&m);
        assert!(res.is_ok());
        assert_eq!(
            res.unwrap(),
            indoc!(
                r#"---
                id: "0000000000000000000000000000000000000000"
                path: /home/example"#
            )
        );
    }
}
