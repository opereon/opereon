use super::*;
use kg_diag::{BasicDiag, Diag};

//pub type RuntimeError = BasicDiag;
pub type RuntimeResult<T> = Result<T, RuntimeError>;

#[derive(Debug)]
pub enum RuntimeError {
    //    #[display(fmt = "task cancelled by user")]
    Cancelled,

    // FIXME ws to be removed
    //    #[display(fmt = "io")]
    Io,
    // FIXME ws to be removed
    //    #[display(fmt = "custom")]
    Custom,
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
