use crate::{
    AsScoped, EngineRef, Host, ModelPath, OperationImpl, OperationRef, Outcome, RuntimeError,
    RuntimeResult, SourceRef,
};
use kg_tree::opath::Opath;
use op_model::{HostDef, ModelDef, ParsedModelDef, ScopedModelDef};
use serde::export::fmt::Debug;
use slog::Logger;
use std::io::BufReader;
use std::sync::{Arc, Mutex};
use tokio::prelude::*;
use tokio::prelude::{Async, Future, Poll};
use tokio_process::Child;

type ChildFuture = Box<dyn Future<Item = String, Error = RuntimeError> + Send>;

/// Operation executing command on hosts specified by `expr`.
pub struct RemoteCommandOperation {
    engine: EngineRef,
    command: String,
    expr: String,
    model_path: ModelPath,

    hosts: Vec<Host>,
    futures: Arc<Mutex<Vec<(Option<ChildFuture>, Option<RuntimeResult<String>>)>>>,
    started: bool,
    logger: Logger,
}

impl Debug for RemoteCommandOperation {
    fn fmt(&self, _f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        unimplemented!()
    }
}

impl RemoteCommandOperation {
    pub fn new(
        operation: OperationRef,
        engine: EngineRef,
        expr: String,
        command: String,
        model_path: ModelPath,
    ) -> RemoteCommandOperation {
        let label = operation.read().label().to_string();
        let logger = engine.read().logger().new(o!(
            "label"=> label,
            "command" => command.clone(),
            "expr" => expr.clone(),
            "model" => model_path.clone(),
        ));
        RemoteCommandOperation {
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

    /// Returns hosts matching `self.expr`
    fn resolve_hosts(&self) -> RuntimeResult<Vec<Host>> {
        let mut e = self.engine.write();
        let m = e.model_manager_mut().resolve(&self.model_path)?;

        // FIXME ws error handling
        let expr = Opath::parse(&self.expr).expect("Cannot parse hosts expression!");
        let hosts_nodes = {
            let m = m.lock();
            kg_tree::set_base_path(m.metadata().path());
            let scope = m.scope()?;
            expr.apply_ext(m.root(), m.root(), &scope)?
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

    /// Creates future collecting child output.
    fn get_child_future(mut child: Child, host: &Host) -> ChildFuture {
        // consume child stdout and stderr
        let stdout = child.stdout().take().expect("Cannot get child stdout");
        let stderr = child.stderr().take().expect("Cannot get child stderr");

        let h = Arc::new(host.hostname().to_string());

        // for now skip child exit status
        let child_fut = child
            .map_err(|_err| RuntimeError::Custom)
            .map(|_exit_status| ());

        // format stdout/stderr lines
        let hostname = h.clone();
        let stdout_fut = tokio::io::lines(BufReader::new(stdout))
            .map(move |line| format!("[{}] out: {}", hostname, line));
        let hostname = h.clone();
        let stderr_fut = tokio::io::lines(BufReader::new(stderr))
            .map(move |line| format!("[{}] err: {}", hostname, line));

        // stdout and stderr as single stream
        let out_fut = stdout_fut
            .select(stderr_fut)
            .map_err(|err| RuntimeError::from(err))
            .collect()
            .map(|res| res.join("\n")); // collect output as single string

        // map errors and resolve with collected output when child finishes
        let fut = child_fut
            .join(out_fut)
            .map_err(|_err| RuntimeError::Custom)
            .map(|(_, out)| out);

        Box::new(fut)
    }

    /// Initialize inner futures and make a first poll
    fn start_polling(&mut self) -> Poll<Outcome, RuntimeError> {
        self.hosts = self.resolve_hosts()?;
        let mut futs = Vec::with_capacity(self.hosts.len());
        info!(self.logger, "Executing command [{command}] on hosts: \n{hosts}", command=self.command.clone(), hosts=format!("{:#?}", self.hosts); "verbosity" => 1);
        for host in &self.hosts {
            // FIXME ssh_session_cache_mut().get(..) is blocking call, should be implemented as Future
            match self
                .engine
                .write()
                .ssh_session_cache_mut()
                .get(host.ssh_dest())
            {
                Ok(session) => {
                    let child = session.read().run_script_async(
                        SourceRef::Source(&self.command),
                        &[],
                        None,
                        None,
                        None,
                    )?;
                    let fut = Self::get_child_future(child, &host);
                    futs.push((Some(fut), None));
                }
                Err(err) => {
                    futs.push((None, Some(Err(RuntimeError::from(err)))));
                }
            }
        }
        *self.futures.lock().unwrap() = futs;
        self.started = true;
        self.poll()
    }

    /// Consume inner futures results, log them and return `Async::Ready`
    fn finish_polling(&mut self) -> Poll<Outcome, RuntimeError> {
        let res: Vec<(String, RuntimeResult<String>)> = self
            .hosts
            .iter()
            .zip(self.futures.lock().unwrap().iter_mut())
            .map(|(host, (_, result))| (host.hostname().to_string(), result.take().unwrap()))
            .collect();
        info!(self.logger, "Finished executing command on remote hosts!"; "verbosity" => 0);
        for (h, out) in res.iter() {
            match out {
                Ok(out) => {
                    info!(self.logger, "================Host [{host}]================\n{out}", host=h, out=out; "verbosity" => 0);
                }
                Err(err) => {
                    info!(self.logger, "================Host [{host}]================\nRemote command execution failed: {err}", host=h, err=format!("{}", err); "verbosity" => 0);
                }
            }
        }
        Ok(Async::Ready(Outcome::Empty))
    }
}

impl Future for RemoteCommandOperation {
    type Item = Outcome;
    type Error = RuntimeError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        if !self.started {
            self.start_polling()
        } else {
            let mut finished = true;
            // Poll ChildFutures and collect results
            for (fut, result) in self.futures.lock().unwrap().iter_mut() {
                if result.is_some() || fut.is_none() {
                    continue;
                }
                match fut.as_mut().unwrap().poll() {
                    Ok(Async::Ready(output)) => *result = Some(Ok(output)),
                    Ok(Async::NotReady) => finished = false,
                    Err(err) => *result = Some(Err(RuntimeError::from(err))),
                }
            }

            if finished {
                self.finish_polling()
            } else {
                Ok(Async::NotReady)
            }
        }
    }
}

impl OperationImpl for RemoteCommandOperation {
    fn init(&mut self) -> RuntimeResult<()> {
        Ok(())
    }
}
