use super::*;

#[derive(Debug)]
pub struct DirManager {
    path: PathBuf,
}

impl FileVersionManager for DirManager {
    fn resolve(&mut self, _rev_path: &RevPath) -> Result<Oid, BasicDiag> {
        unimplemented!()
    }

    fn checkout(&mut self, _rev_id: Oid) -> Result<RevInfo, BasicDiag> {
        unimplemented!()
    }

    fn commit(&mut self, _message: &str) -> Result<Oid, BasicDiag> {
        unimplemented!()
    }

    fn get_file_diff(&mut self, _old_rev_id: Oid, _new_rev_id: Oid) -> Result<FileDiff, BasicDiag> {
        unimplemented!()
    }
}