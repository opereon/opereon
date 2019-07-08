use super::*;
use crate::{ProtoError, FileError};
use kg_diag::{Diag, BasicDiag};

//FIXME (jc)
#[derive(Debug, Detail)]
#[diag(code_offset = 500)]
pub enum RuntimeError {
    #[diag(code = 1)]
    Cancelled,
    #[diag(code = 2)]
    Io,
//    Io(Box<kg_diag::Diag>),
    #[diag(code = 3)]
    Custom,
}

impl Diag for RuntimeError {
    fn cause(&self) -> Option<&dyn Diag> {
        unimplemented!()
    }

    fn cause_mut(&mut self) -> Option<&mut dyn Diag> {
        unimplemented!()
    }
}


//FIXME (jc)
impl From<std::io::Error> for RuntimeError {
    fn from(err: std::io::Error) -> Self {
        println!("io err: {}", err);
        RuntimeError::Io
    }
}

//FIXME (jc)
impl From<kg_io::IoError> for RuntimeError {
    fn from(err: kg_io::IoError) -> Self {
        println!("kg_io err: {}", err);
        RuntimeError::Io
    }
}

//FIXME (jc)
impl From<kg_tree::opath::OpathRuntimeError> for RuntimeError {
    fn from(_err: kg_tree::opath::OpathRuntimeError) -> Self {
        println!("opath err");
        RuntimeError::Custom
    }
}

//FIXME (jc)
impl From<ProtoError> for RuntimeError {
    fn from(_err: ProtoError) -> Self {
        println!("proto err");
        RuntimeError::Custom
    }
}

//FIXME (jc)
impl From<CommandError> for RuntimeError {
    fn from(_err: CommandError) -> Self {
        println!("command err");
        RuntimeError::Custom
    }
}

//FIXME (jc)
impl From<FileError> for RuntimeError {
    fn from(_err: FileError) -> Self {
        println!("file err");
        RuntimeError::Custom
    }
}

//FIXME (jc)
impl From<kg_diag::BasicDiag> for RuntimeError {
    fn from(_err: BasicDiag) -> Self {
        println!("basic diag err");
        RuntimeError::Custom
    }
}


impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}
