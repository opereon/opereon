use crate::outcome::Outcome;
use crate::services::model_manager::ModelManager;
use crate::state::CoreState;
use async_trait::*;
use kg_diag::DiagResultExt;
use kg_diag::Severity;
use kg_tree::diff::NodeDiff;
use kg_tree::opath::Opath;
use kg_tree::serial::to_tree;
use op_engine::operation::OperationResult;
use op_engine::{EngineRef, OperationImpl, OperationRef};
use op_model::{ModelDef, ScopedModelDef};
use op_rev::RevPath;
use std::path::PathBuf;

#[derive(Debug, Detail, Display)]
pub enum ModelOpErrorDetail {
    #[display(fmt = "cannot query model")]
    QueryOp,
}

#[derive(Debug)]
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
    #[instrument(
    name = "ModelQueryOperation",
    skip(self, engine, _operation),
    fields(
        model_path = % _self.model_path,
        expr = % _self.expr)
    )]
    async fn done(
        &mut self,
        engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<Outcome> {
        info!(verb=2, "Querying model");
        let mut manager = engine.service::<ModelManager>().await.unwrap();
        let model = manager.resolve(&self.model_path).await?;
        let expr = Opath::parse(&self.expr).map_err_as_cause(|| ModelOpErrorDetail::QueryOp)?;

        let res = {
            let m = model.lock();
            kg_tree::set_base_path(m.rev_info().path());
            let scope = m.scope()?;
            expr.apply_ext(m.root(), m.root(), &scope)?
        };

        Ok(Outcome::NodeSet(res.into()))
    }
}

pub struct ModelCommitOperation {
    message: String,
}

impl ModelCommitOperation {
    pub fn new(message: String) -> Self {
        ModelCommitOperation { message }
    }
}

#[async_trait]
impl OperationImpl<Outcome> for ModelCommitOperation {
    async fn done(
        &mut self,
        engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<Outcome> {
        let mut manager = engine.service::<ModelManager>().await.unwrap();
        let _m = manager.commit(&self.message).await?;
        Ok(Outcome::Empty)
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
        let model = manager.resolve(&self.model_path).await?;
        let res = to_tree(&*model.lock()).unwrap();
        Ok(Outcome::NodeSet(res.into()))
    }
}

pub struct ModelDiffOperation {
    source: RevPath,
    target: RevPath,
}

impl ModelDiffOperation {
    pub fn new(source: RevPath, target: RevPath) -> Self {
        ModelDiffOperation { source, target }
    }
}

#[async_trait]
impl OperationImpl<Outcome> for ModelDiffOperation {
    async fn done(
        &mut self,
        engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<Outcome> {
        let mut manager = engine.service::<ModelManager>().await.unwrap();
        let m1 = manager.resolve(&self.source).await?;
        let m2 = manager.resolve(&self.target).await?;
        let state = engine.state::<CoreState>().unwrap();
        let diff = {
            NodeDiff::diff(
                m1.lock().root(),
                m2.lock().root(),
                state.config().model().diff(),
            )
        };

        Ok(Outcome::NodeSet(to_tree(&diff).unwrap().into()))
    }
}

pub struct ModelInitOperation {
    path: PathBuf,
}

impl ModelInitOperation {
    pub fn new(path: PathBuf) -> Self {
        ModelInitOperation { path }
    }
}

#[async_trait]
impl OperationImpl<Outcome> for ModelInitOperation {
    async fn done(
        &mut self,
        engine: &EngineRef<Outcome>,
        _operation: &OperationRef<Outcome>,
    ) -> OperationResult<Outcome> {
        let mut manager = engine.service::<ModelManager>().await.unwrap();
        manager.create_model(self.path.clone()).await?;
        Ok(Outcome::Empty)
    }
}
