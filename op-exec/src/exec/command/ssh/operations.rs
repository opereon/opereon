use crate::{OperationRef, EngineRef, OperationImpl, RuntimeError, Outcome, ModelPath, Host, AsScoped, create_command_executor, SourceRef};
use slog::Logger;
use tokio::prelude::{Future, Poll, Async};
use kg_tree::opath::Opath;
use op_model::{HostDef, ScopedModelDef, ModelDef, ParsedModelDef, Run};
use std::sync::{Arc, Mutex};
use tokio_process::WaitWithOutput;

/// Operation executing command on hosts specified by `expr`.
#[derive(Debug)]
pub struct RemoteCommandOperation {
    operation: OperationRef,
    engine: EngineRef,
    command: String,
    expr: String,
    model_path: ModelPath,

    hosts: Vec<Host>,
    futures: Arc<Mutex<Vec<(Option<WaitWithOutput>, Option<Result<String, RuntimeError>>)>>>,
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
            futures: Arc::new(Mutex::new(vec![])),
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
}

impl Future for RemoteCommandOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if !self.started {
            self.hosts = self.resolve_hosts()?;
            let mut futs = Vec::with_capacity(self.hosts.len());
            info!(self.logger, "Executing command [{command}] on hosts: \n{hosts}", command=self.command.clone(), hosts=format!("{:#?}", self.hosts); "verbosity" => 1);

            for host in &self.hosts {

                match self.engine.write().ssh_session_cache_mut().get(host.ssh_dest()) {
                    Ok(mut session) => {
                        let child = session.read().run_script_async(SourceRef::Source(&self.command), &[], None, None, None)?;
                        futs.push((Some(child.wait_with_output()), None));
                    }
                    Err(err) => {
                        futs.push((None, Some(Err(RuntimeError::from(err)))));
                    }
                }

            }

            *self.futures.lock().unwrap() = futs;

            self.started = true;
            self.poll()
        } else {
            let mut finished = true;
            for (fut, result) in self.futures.lock().unwrap().iter_mut() {
                if result.is_some() || fut.is_none(){
                    continue
                }
                match fut.as_mut().unwrap().poll() {
                    Ok(Async::Ready(output)) => {
                        *result = Some(Ok(String::from_utf8(output.stdout).unwrap()))
                    },
                    Ok(Async::NotReady) => {
                        finished = false
                    },
                    Err(err) => {
                        *result = Some(Err(RuntimeError::from(err)))
                    },
                }
            }

            if finished {
                let res: Vec<(String, Result<String, RuntimeError>)> = self.hosts.iter()
                    .zip(self.futures.lock().unwrap().iter_mut())
                    .map(|(host, (_, result))| {
                        (host.hostname().to_string(), result.take().unwrap())
                    })
                    .collect();

                for (h, out) in res.iter() {
                    match out {
                        Ok(out) => {
                            eprintln!("Host {}\n{}", h, out);
                        },
                        Err(err) => {
                            eprintln!("Host {}\n Error: {:?}", h, err);
                        },
                    }
                }
                Ok(Async::Ready(Outcome::Empty))
            } else {
                Ok(Async::NotReady)
            }
        }
    }
}

impl OperationImpl for RemoteCommandOperation {
    fn init(&mut self) -> Result<(), RuntimeError> {
        Ok(())
    }
}