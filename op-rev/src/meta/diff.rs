use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    kind: ChangeKind,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    old_path: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    new_path: Option<PathBuf>,
}

impl FileChange {
    pub fn new(kind: ChangeKind, old_path: Option<PathBuf>, new_path: Option<PathBuf>) -> Self {
        FileChange {
            kind,
            old_path,
            new_path,
        }
    }

    pub fn kind(&self) -> ChangeKind {
        self.kind
    }

    pub fn old_path(&self) -> Option<&PathBuf> {
        self.old_path.as_ref()
    }

    pub fn new_path(&self) -> Option<&PathBuf> {
        self.new_path.as_ref()
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiff {
    changes: Vec<FileChange>,
}

impl FileDiff {
    pub fn new(changes: Vec<FileChange>) -> Self {
        FileDiff {
            changes
        }
    }

    pub fn changes(&self) -> &Vec<FileChange> {
        &self.changes
    }
}
