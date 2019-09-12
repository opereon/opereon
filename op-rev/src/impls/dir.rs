use super::*;

#[derive(Debug)]
pub struct DirManager {
    path: PathBuf,
}

impl FileVersionManager for DirManager {
    fn resolve(&mut self, rev_path: &RevPath) -> Result<Oid, BasicDiag> {
        unimplemented!()
    }

    fn checkout(&mut self, rev_id: Oid) -> Result<RevInfo, BasicDiag> {
        unimplemented!()
    }

    fn commit(&mut self, message: &str) -> Result<Oid, BasicDiag> {
        unimplemented!()
    }

    fn get_file_diff(&mut self, old_rev_id: Oid, new_rev_id: Oid) -> Result<FileDiff, BasicDiag> {
        unimplemented!()
    }
}