use kg_template::parse::Parser;

use super::*;

pub struct TemplateResolver {
    parser: Parser,
}

impl TemplateResolver {
    pub (super) fn new(engine: EngineRef) -> TemplateResolver {
        let cfg = engine.read().config().exec().template().kg().clone();
        TemplateResolver {
            parser: Parser::with_config(cfg),
        }
    }
}

impl TemplateExecutor for TemplateResolver {
    fn process_template(&mut self, _engine: &EngineRef, task: &TaskDef, src_path: &Path, dst_path: &Path, _log: &OutputLog) -> Result<TaskResult, RuntimeError> {
        let template = {
            let mut f = FileBuffer::open(src_path)?;
            let mut r = f.char_reader();
            match self.parser.parse(&mut r) {
                Ok(t) => t,
                Err(err) => {
                    //FIXME (jc)
                    eprintln!("Error parsing template: {:?}", err);
                    return Err(RuntimeError::Custom);
                }
            }
        };

        let mut res = String::new();
        template.render_ext(task.root(), task.node(), task.scope(), &mut res).unwrap(); //FIXME (jc)

        if let Some(p) = dst_path.parent() {
            std::fs::create_dir_all(p)?;
        }
        std::fs::write(&dst_path, res)?;

        Ok(TaskResult::new(Outcome::Empty, Some(0), None))
    }
}

