use crate::outcome::Outcome;
use crate::services::model_manager::ModelManager;
use async_trait::*;
use kg_diag::DiagResultExt;
use kg_diag::Severity;
use kg_tree::opath::Opath;
use kg_tree::serial::to_tree;
use op_engine::operation::OperationResult;
use op_engine::{EngineRef, OperationImpl, OperationRef};
use op_model::{ModelDef, ScopedModelDef};
use op_rev::RevPath;

#[derive(Debug, Detail, Display)]
pub enum ModelOpErrorDetail {
    #[display(fmt = "cannot query model")]
    QueryOp,
}

pub struct ModelQueryOperation {
    model_path: RevPath,
    expr: String,
}

impl ModelQueryOperation {
    pub fn new(model_path: RevPath, expr: String) -> Self {
        ModelQueryOperation { model_path, expr }
    }
}

#[async_trait]
impl OperationImpl<Outcome> for ModelQueryOperation {
    async fn done(
        &mut self,
        engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<Outcome> {
        let mut manager = engine.service::<ModelManager>().await.unwrap();
        let model = manager.resolve(&self.model_path)?;
        let expr = Opath::parse(&self.expr).map_err_as_cause(|| ModelOpErrorDetail::QueryOp)?;

        // info!(self.logger, "Querying model...");
        let res = {
            let m = model.lock();
            kg_tree::set_base_path(m.rev_info().path());
            let scope = m.scope()?;
            expr.apply_ext(m.root(), m.root(), &scope)?
        };

        Ok(Outcome::NodeSet(res.into()))
    }
}

pub struct ModelTestOperation {
    model_path: RevPath,
}

impl ModelTestOperation {
    pub fn new(model_path: RevPath) -> Self {
        ModelTestOperation { model_path }
    }
}

#[async_trait]
impl OperationImpl<Outcome> for ModelTestOperation {
    async fn done(
        &mut self,
        engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<Outcome> {
        let mut manager = engine.service::<ModelManager>().await.unwrap();
        let model = manager.resolve(&self.model_path)?;
        let res = to_tree(&*model.lock()).unwrap();
        Ok(Outcome::NodeSet(res.into()))
    }
}
