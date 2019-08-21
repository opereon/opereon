use super::*;
use kg_diag::{BasicDiag, Diag};

pub type RuntimeResult<T> = Result<T, RuntimeError>;

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
impl From<kg_diag::IoErrorDetail> for RuntimeError {
    fn from(err: kg_diag::IoErrorDetail) -> Self {
        println!("kg_io err: {}", err);
        RuntimeError::Io
    }
}

//FIXME (jc)
impl From<kg_tree::opath::ExprErrorDetail> for RuntimeError {
    fn from(_err: kg_tree::opath::ExprErrorDetail) -> Self {
        println!("opath err");
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
    fn from(err: BasicDiag) -> Self {
        println!("basic diag err: {}", err);
        RuntimeError::Custom
    }
}

//FIXME ws
impl From<std::fmt::Error> for RuntimeError {
    fn from(err: std::fmt::Error) -> Self {
        println!("fmt err {:?}", err);
        RuntimeError::Custom
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}
