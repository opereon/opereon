use globset::{Glob, GlobBuilder};
use serde::Serializer;

use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct ModelWatch {
    path: Opath,
    mask: ChangeKindMask,
}

impl ModelWatch {
    pub fn parse(path: &str, mask: &str) -> DefsResult<ModelWatch> {
        Ok(ModelWatch {
            path: Opath::parse(path).map_err_as_cause(|| DefsErrorDetail::ProcModelWatchParse)?,
            mask: ChangeKindMask::parse(mask),
        })
    }

    pub fn path(&self) -> &Opath {
        &self.path
    }

    pub fn mask(&self) -> ChangeKindMask {
        self.mask
    }
}

fn glob_serialize<S>(glob: &Glob, s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_str(glob.glob())
}

#[derive(Debug, Clone, Serialize)]
pub struct FileWatch {
    #[serde(serialize_with = "glob_serialize")]
    glob: Glob,
    mask: ChangeKindMask,
}

impl FileWatch {
    pub fn parse(glob: &str, mask: &str) -> DefsResult<FileWatch> {
        let glob = GlobBuilder::new(glob)
            .build()
            .map_err(|err| DefsErrorDetail::ProcFileWatchParse { err })?;
        Ok(FileWatch {
            glob,
            mask: ChangeKindMask::parse(mask),
        })
    }

    pub fn glob(&self) -> &Glob {
        &self.glob
    }

    pub fn mask(&self) -> ChangeKindMask {
        self.mask
    }
}
