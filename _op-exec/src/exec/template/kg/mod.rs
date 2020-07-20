use kg_template::parse::Parser;

use super::TemplateErrorDetail::*;
use super::*;

pub struct TemplateResolver {
    parser: Parser,
}

impl TemplateResolver {
    pub(super) fn new(engine: EngineRef) -> TemplateResolver {
        let cfg = engine.read().config().exec().template().kg().clone();
        TemplateResolver {
            parser: Parser::with_config(cfg),
        }
    }
}

impl TemplateExecutor for TemplateResolver {
    fn process_template(
        &mut self,
        _engine: &EngineRef,
        task: &TaskDef,
        src_path: &Path,
        dst_path: &Path,
        _log: &OutputLog,
    ) -> TemplateResult<TaskResult> {
        let template = {
            let f = FileBuffer::open(src_path)
                .into_diag_res()
                .map_err_as_cause(|| Open)?;
            let mut r = f.char_reader();

            self.parser.parse(&mut r).map_err_as_cause(|| Parse {
                file: src_path.to_string_lossy().to_string(),
            })?
        };

        let mut res = String::new();
        template
            .render_ext(task.root(), task.node(), task.scope()?, &mut res)
            .map_err_as_cause(|| Render {
                file: src_path.to_string_lossy().to_string(),
            })?;

        if let Some(p) = dst_path.parent() {
            fs::create_dir_all(p).into_diag_res().map_err_as_cause(|| {
                TemplateErrorDetail::Write {
                    dst_file: dst_path.to_string_lossy().to_string(),
                }
            })?;
        }
        fs::write(&dst_path, res)
            .into_diag_res()
            .map_err_as_cause(|| TemplateErrorDetail::Write {
                dst_file: dst_path.to_string_lossy().to_string(),
            })?;

        Ok(TaskResult::new(Outcome::Empty, Some(0), None))
    }
}
