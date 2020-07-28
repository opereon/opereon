use super::*;


#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type", content = "arg")]
pub enum RevPath {
    /// Current working directory
    Current,
    /// Named revision, usually a Git revision string
    /// (see http://git-scm.com/docs/git-rev-parse.html#_specifying_revisions)
    Revision(String),
}

impl std::fmt::Display for RevPath {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            RevPath::Current => write!(f, "@"),
            RevPath::Revision(ref id) => write!(f, "id: {}", id),
        }
    }
}

impl std::str::FromStr for RevPath {
    type Err = String;

    fn from_str(s: &str) -> Result<RevPath, Self::Err> {
        Ok(match s {
            "@" | "@current" => RevPath::Current,
            _ => RevPath::Revision(s.to_string()),
        })
    }
}

impl From<Oid> for RevPath {
    fn from(oid: Oid) -> Self {
        RevPath::Revision(oid.to_string())
    }
}
