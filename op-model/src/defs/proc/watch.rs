use super::*;


#[derive(Debug, Clone, Serialize)]
pub struct Watch {
    path: Opath,
    mask: ChangeKindMask,
}

impl Watch {
    pub fn parse(path: &str, mask: &str) -> Result<Watch, DefsParseError> {
        //FIXME (jc) handle opath parse errors
        Ok(Watch {
            path: Opath::parse(path).unwrap(),
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
