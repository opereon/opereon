use super::*;
use kg_diag::io::fs;

#[derive(Debug)]
pub struct DirManager {
    path: PathBuf,
}

impl DirManager {
    /// Open existing folder repository and return `DirManager`
    pub fn open<P: Into<PathBuf> + AsRef<Path>>(repo_dir: P) -> Result<Self, BasicDiag> {
        let path = fs::canonicalize(repo_dir.as_ref())?;
        if path.is_dir() {
            Ok(DirManager {
                path,
            })
        } else {
            Err(IoErrorDetail::file_not_found(path, FileType::Dir, OpType::Read).into())
        }
    }

    /// Create a new folder repository and return `DirManager`
    pub fn create<P: Into<PathBuf> + AsRef<Path>>(repo_dir: P) -> Result<Self, BasicDiag> {
        let path = fs::canonicalize(repo_dir.as_ref())?;
        fs::create_dir_all(&path)?;
        Ok(DirManager {
            path,
        })
    }
}
#[async_trait]
impl FileVersionManager for DirManager {
    async fn resolve(&mut self, rev_path: &RevPath) -> Result<Oid, BasicDiag> {
        match *rev_path {
            RevPath::Current => Ok(Oid::nil()),
            RevPath::Revision(ref spec) => {
                let oid: Oid = spec.parse().map_err(|_| IoErrorDetail::Io {
                    kind: std::io::ErrorKind::InvalidInput,
                    message: "Cannot parse Oid".into()
                })?;
                let mut path = self.path.join(".op");
                path.push(&format!("{:12}", oid));
                let m = fs::metadata(&path)?;
                if m.is_dir() {
                    Ok(oid)
                } else {
                    //FIXME (jc) create custom error
                    Err(IoErrorDetail::file_not_found(path, FileType::Dir, OpType::Read).into())
                }
            }
        }
    }

    async fn checkout(&mut self, rev_id: Oid) -> Result<RevInfo, BasicDiag> {
        if rev_id.is_nil() {
            Ok(RevInfo::new(rev_id, self.path.clone()))
        } else {
            let mut checkout_path = self.path.join(".op/revs");
            checkout_path.push(&format!("{:12}", rev_id));

            if checkout_path.is_dir() {
                Ok(RevInfo::new(rev_id, checkout_path))
            } else {
                //FIXME (jc) create custom error
                Err(IoErrorDetail::file_not_found(self.path.clone(), FileType::Dir, OpType::Read).into())
            }
        }
    }

    async fn commit(&mut self, _message: &str) -> Result<Oid, BasicDiag> {
        unimplemented!()
    }

    async fn get_file_diff(&mut self, _old_rev_id: Oid, _new_rev_id: Oid) -> Result<FileDiff, BasicDiag> {
        unimplemented!()
    }
}