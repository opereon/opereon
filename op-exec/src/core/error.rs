use kg_diag::BasicDiag;
use kg_diag::Severity;

pub type RuntimeError = BasicDiag;
pub type RuntimeResult<T> = Result<T, RuntimeError>;

#[derive(Debug, Display, Detail)]
pub enum RuntimeErrorDetail {
    #[display(fmt = "task cancelled by user")]
    Cancelled,
}
