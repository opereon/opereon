use kg_diag::BasicDiag;
use kg_diag::Severity;

pub type ProtoError = BasicDiag;
pub type ProtoResult<T> = Result<T, ProtoError>;

#[derive(Debug, Display, Detail)]
pub enum ProtoErrorDetail {
    #[display(fmt = "cannot parse host")]
    HostParse,

    #[display(fmt = "cannot create step exec")]
    StepExecCreate,

    #[display(fmt = "cannot create proc exec dir")]
    ProcExecDir,

    #[display(fmt = "cannot load proc exec from '{file_path}'")]
    ProcExecLoad { file_path: String },
}
