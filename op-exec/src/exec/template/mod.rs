use super::*;

mod config;
mod kg;

pub use self::config::TemplateConfig;


pub trait TemplateExecutor {
    fn process_template(&mut self, engine: &EngineRef, task: &TaskDef, src_path: &Path, dst_path: &Path, log: &OutputLog) -> Result<TaskResult, RuntimeError>;
}


pub fn create_template_executor(_host: &Host, engine: &EngineRef) -> Result<Box<dyn TemplateExecutor>, RuntimeError> {
    Ok(Box::new(self::kg::TemplateResolver::new(engine.clone())))
}
