use super::*;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase", tag = "type", content = "arg")]
pub enum ModelPath {
    /// Current working directory
    Current,
    /// Git revision string http://git-scm.com/docs/git-rev-parse.html#_specifying_revisions
    Revision(String),
}


pub trait FileVersionManager {
    fn init(&mut self, path: &Path) -> Result<(), BasicDiag>;

    fn checkout(&mut self, path: &Path, model_path: ModelPath) -> Result<Metadata, BasicDiag>;

    fn commit(&mut self, path: &Path, message: &str) -> Result<Metadata, BasicDiag>;

    fn read_file_into(&mut self, path: &Path, buffer: &mut Vec<u8>) -> Result<(), BasicDiag>;

    fn read_file(&mut self, path: &Path) -> Result<Vec<u8>, BasicDiag> {
        let mut buff = Vec::new();
        self.read_file_into(path, &mut buff)?;
        Ok(buff)
    }
}

