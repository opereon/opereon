use super::*;

pub use self::config::TemplateConfig;

mod config;
mod kg;

pub trait TemplateExecutor {
    fn process_template(
        &mut self,
        engine: &EngineRef,
        task: &TaskDef,
        src_path: &Path,
        dst_path: &Path,
        log: &OutputLog,
    ) -> RuntimeResult<TaskResult>;
}

pub fn create_template_executor(
    _host: &Host,
    engine: &EngineRef,
) -> RuntimeResult<Box<dyn TemplateExecutor>> {
    Ok(Box::new(self::kg::TemplateResolver::new(engine.clone())))
}
