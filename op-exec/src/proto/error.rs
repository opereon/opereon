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

impl From<kg_io::error::IoError> for ProtoError {
    fn from(err: kg_io::error::IoError) -> Self {
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

impl From<kg_tree::opath::OpathRuntimeError> for ProtoError {
    fn from(err: kg_tree::opath::OpathRuntimeError) -> Self {
        eprintln!("{:?}", err);
        unimplemented!()
    }
}

impl From<op_model::DefsParseError> for ProtoError {
    fn from(err: op_model::DefsParseError) -> Self {
        eprintln!("{:?}", err);
        unimplemented!()
    }
}
