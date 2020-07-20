use super::*;

pub use self::config::TemplateConfig;

mod config;
mod kg;

pub type TemplateError = BasicDiag;
pub type TemplateResult<T> = Result<T, TemplateError>;

#[derive(Debug, Display, Detail)]
pub enum TemplateErrorDetail {
    #[display(fmt = "cannot open template file")]
    Open,

    #[display(fmt = "cannot parse template file: '{file}'")]
    Parse { file: String },

    #[display(fmt = "cannot render template file: '{file}'")]
    Render { file: String },

    #[display(fmt = "cannot write template file to: '{dst_file}'")]
    Write { dst_file: String },
}

pub trait TemplateExecutor {
    fn process_template(
        &mut self,
        engine: &EngineRef,
        task: &TaskDef,
        src_path: &Path,
        dst_path: &Path,
        log: &OutputLog,
    ) -> TemplateResult<TaskResult>;
}

pub fn create_template_executor(
    _host: &Host,
    engine: &EngineRef,
) -> TemplateResult<Box<dyn TemplateExecutor>> {
    Ok(Box::new(self::kg::TemplateResolver::new(engine.clone())))
}
