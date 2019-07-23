use kg_diag::BasicDiag;

//FIXME (jc)
#[derive(Debug, Clone)]
pub enum ProtoError {
    ParseDef(String),
}

impl From<std::io::Error> for ProtoError {
    fn from(err: std::io::Error) -> Self {
        eprintln!("{}", err);
        unimplemented!()
    }
}

impl From<kg_diag::IoError> for ProtoError {
    fn from(err: kg_diag::IoError) -> Self {
        eprintln!("{}", err);
        unimplemented!()
    }
}

impl From<kg_tree::serial::Error> for ProtoError {
    fn from(err: kg_tree::serial::Error) -> Self {
        eprintln!("{}", err);
        unimplemented!()
    }
}

impl From<kg_tree::opath::OpathParseError> for ProtoError {
    fn from(err: kg_tree::opath::OpathParseError) -> Self {
        eprintln!("{}", err);
        unimplemented!()
    }
}

impl From<kg_tree::opath::ExprErrorDetail> for ProtoError {
    fn from(err: kg_tree::opath::ExprErrorDetail) -> Self {
        eprintln!("{:?}", err);
        unimplemented!()
    }
}

impl From<op_model::DefsErrorDetail> for ProtoError {
    fn from(err: op_model::DefsErrorDetail) -> Self {
        eprintln!("{:?}", err);
        unimplemented!()
    }
}

impl From<BasicDiag> for ProtoError {
    fn from(err: BasicDiag) -> Self {
        eprintln!("{:?}", err);
        unimplemented!()
    }
}
