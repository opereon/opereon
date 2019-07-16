use crate::{OperationRef, EngineRef, OperationImpl, RuntimeError, Outcome, ModelPath, Host, AsScoped, create_command_executor};
use slog::Logger;
use tokio::prelude::{Future, Poll, Async};
use kg_tree::opath::Opath;
use op_model::{HostDef, ScopedModelDef, ModelDef, ParsedModelDef, Run};
use std::sync::{Arc, Mutex};

/// Operation executing command on hosts specified by `expr`.
#[derive(Debug)]
pub struct RemoteCommandOperation {
    operation: OperationRef,
    engine: EngineRef,
    command: String,
    expr: String,
    model_path: ModelPath,
    hosts: Vec<Host>,
    results: Arc<Mutex<Vec<Result<String, RuntimeError>>>>,
    started: bool,
    logger: Logger,
}

impl RemoteCommandOperation {
    pub fn new(operation: OperationRef, engine: EngineRef, expr: String, command: String, model_path: ModelPath) -> RemoteCommandOperation {
        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "command" => command.clone(),
            "expr" => expr.clone(),
            "model" => model_path.clone(),
        ));
        RemoteCommandOperation {
            operation,
            engine,
            command,
            expr,
            model_path,
            hosts: vec![],
            results: Arc::new(Mutex::new(vec![])),
            started: false,
            logger,
        }
    }

    fn resolve_hosts(&self) -> Result<Vec<Host>, RuntimeError> {
        let mut e = self.engine.write();
        let m = e.model_manager_mut().resolve(&self.model_path)?;

        // FIXME ws error handling
        let expr = Opath::parse(&self.expr).expect("Cannot parse hosts expression!");
        let hosts_nodes = {
            let m = m.lock();
            kg_tree::set_base_path(m.metadata().path());
            let scope = m.scope();
            expr.apply_ext(m.root(), m.root(), &scope)
        };

        let mut hosts = Vec::new();
        let m = m.lock();

        for h in hosts_nodes.iter() {
            // FIXME ws error handling
            let hd = HostDef::parse(&*m, m.as_scoped(), h).expect("Cannot parse host definition!");
            hosts.push(Host::from_def(&hd)?);
        }
        Ok(hosts)
    }

    fn start_execution(&mut self) -> Result<(), RuntimeError> {
        self.hosts = self.resolve_hosts()?;

        let mut hosts = self.hosts.clone();
        let e = self.engine.clone();

        std::thread::spawn(move || {
            let mut handles = Vec::with_capacity(hosts.len());

            for host in hosts.drain(..) {
                let engine = e.clone();
                let jh = std::thread::spawn(move || {
                    let inner = move || -> Result<String, RuntimeError>{
                        let e = engine.write().ssh_session_cache_mut().get(host.ssh_dest())?;

                    };



                });

                handles.push(jh);
            }

        });



//        let mut executor = create_command_executor(step_exec.host(), &self.engine)?;
//        executor.exec_command(&self.engine,
//                              &self.command,
//                              &[],
//                              out_format,
//                              &output)?




        self.started = true;

        info!(self.logger, "Executing command [{command}] on hosts: \n{hosts}", command=self.command.clone(), hosts=format!("{:#?}", self.hosts); "verbosity" => 1);

        Ok(())
    }
}

impl Future for RemoteCommandOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if !self.started {
            self.start_execution()?;
            Ok(Async::NotReady)
        } else {
            Ok(Async::Ready(Outcome::Empty))
        }
    }
}

impl OperationImpl for RemoteCommandOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}