use std::collections::VecDeque;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use uuid::Uuid;

use super::*;
use slog::Logger;

#[derive(Debug)]
pub struct Engine {
    config: ConfigRef,
    model_manager: ModelManager,
    exec_manager: ExecManager,
    ssh_session_cache: SshSessionCache,
    operation_queue1: VecDeque<OperationTask>,
    operation_queue2: VecDeque<OperationTask>,
    operations: LinkedHashMap<Uuid, OperationRef>,
    task: AtomicTask,
    stopped: bool,
    logger: slog::Logger,
}

impl Engine {
    pub fn new(model_dir: PathBuf, config: ConfigRef, logger: slog::Logger) -> Engine {
        let model_manager = ModelManager::new(model_dir, config.clone(), logger.clone());
        let exec_manager = ExecManager::new(config.clone());
        let ssh_session_cache = SshSessionCache::new(config.clone());

        Engine {
            config,
            model_manager,
            exec_manager,
            ssh_session_cache,
            operation_queue1: VecDeque::new(),
            operation_queue2: VecDeque::new(),
            operations: LinkedHashMap::new(),
            task: AtomicTask::new(),
            stopped: false,
            logger,
        }
    }

    pub fn config(&self) -> &ConfigRef {
        &self.config
    }

    pub fn model_manager(&self) -> &ModelManager {
        &self.model_manager
    }

    pub fn model_manager_mut(&mut self) -> &mut ModelManager {
        &mut self.model_manager
    }

    pub fn exec_manager(&self) -> &ExecManager {
        &self.exec_manager
    }

    pub fn exec_manager_mut(&mut self) -> &mut ExecManager {
        &mut self.exec_manager
    }

    pub fn ssh_session_cache(&self) -> &SshSessionCache {
        &self.ssh_session_cache
    }

    pub fn ssh_session_cache_mut(&mut self) -> &mut SshSessionCache {
        &mut self.ssh_session_cache
    }

    pub fn notify(&self) {
        self.task.notify();
    }

    pub(super) fn remove_operation(&mut self, operation: &OperationRef) {
        let id = operation.read().id();
        self.operations.remove(&id);
    }

    fn is_done(&self) -> bool {
        self.operations.is_empty()
    }

    fn swap_queues(&mut self) {
        std::mem::swap(&mut self.operation_queue1, &mut self.operation_queue2);
        self.operation_queue2.clear();
    }
    pub fn stop(&mut self) {
        self.stopped = true;
        self.task.notify();

        // TODO save queue etc...
        info!(self.logger, "Stopping engine.");
    }

    pub fn logger(&self) -> &Logger {
        &self.logger
    }
}

#[derive(Debug, Clone)]
pub struct EngineRef(Arc<RwLock<Engine>>);

impl EngineRef {
    pub fn new(model_dir: PathBuf, config: ConfigRef, logger: slog::Logger) -> EngineRef {
        EngineRef(Arc::new(RwLock::new(Engine::new(
            model_dir, config, logger,
        ))))
    }
    pub fn start(
        current_dir: PathBuf,
        config: ConfigRef,
        logger: slog::Logger,
    ) -> IoResult<EngineRef> {
        let engine = EngineRef::new(current_dir, config, logger.clone());
        engine.init_ssh_session_cache()?;
        Ok(engine)
    }

    pub fn read(&self) -> RwLockReadGuard<Engine> {
        self.0.read().unwrap()
    }

    pub fn write(&self) -> RwLockWriteGuard<Engine> {
        self.0.write().unwrap()
    }

    pub fn stop(&self) {
        self.write().stop();
    }

    pub fn init_ssh_session_cache(&self) -> IoResult<()> {
        self.write().ssh_session_cache.init()
    }

    pub fn init_operation_queue(&self) -> IoResult<()> {
        fs::create_dir_all(self.read().config.queue().persist_dir())?;
        self.load_operation_queue()?;
        Ok(())
    }

    fn load_operation_queue(&self) -> IoResult<()> {
        use std::cmp::Ordering;
        use std::time::UNIX_EPOCH;

        let r = {
            let engine = self.read();
            let persist_dir = engine.config.queue().persist_dir();
            info!(engine.logger, "Loading operation queue from"; o!("path"=>persist_dir.display()));

            fs::read_dir(persist_dir)?
        };

        let mut files: Vec<_> = r
            .filter_map(|f| f.ok())
            .map(|f| {
                let path = f.path();
                let timestamp = match f.metadata() {
                    Ok(m) => m.modified().unwrap_or(UNIX_EPOCH),
                    Err(_) => UNIX_EPOCH,
                };
                (path, timestamp)
            })
            .collect();

        files.sort_by(|a, b| match a.1.cmp(&b.1) {
            Ordering::Equal => a.0.cmp(&b.0),
            o @ _ => o,
        });

        for (p, _) in files {
            let s = std::fs::read(p)?;
            let o: OperationRef = rmp_serde::from_slice(&s).unwrap();
            self.enqueue_operation(o, false).unwrap();
        }

        Ok(())
    }

    pub fn enqueue_operation(
        &self,
        operation: OperationRef,
        _persist: bool,
    ) -> RuntimeResult<OutcomeFuture> {
        //        if persist {
        //            operation.persist(self.read().config.queue().persist_dir())?;
        //        }
        let of = OperationTask::new(operation.clone(), self.clone())?;
        self.write().operation_queue1.push_back(of);
        let engine = self.read();
        engine.notify();
        {
            let op = operation.read();
            debug!(engine.logger, "New Operation scheduled";
                        o!(
            //                "context"=> format!("{:?}", op.context()),
                            "id"=> format!("{}", op.id()),
                             "label"=> op.label().to_string(),
                          )
                        );
        }

        Ok(OutcomeFuture::new(operation))
    }

    pub fn block_operation(&self, operation: &OperationRef, block: bool) {
        operation.write().block(block);
        self.read().notify();
    }

    pub fn cancel_operation(&self, operation: &OperationRef) {
        operation.write().cancel();
    }
}

unsafe impl Send for EngineRef {}

unsafe impl Sync for EngineRef {}

impl Future for EngineRef {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        //println!("--- engine poll");

        self.read().task.register();

        {
            let mut e = self.write();
            // do not schedule new operations when engine is stopped
            if !e.stopped {
                while let Some(of) = e.operation_queue1.pop_front() {
                    if of.operation().read().is_blocked() {
                        e.operation_queue2.push_back(of);
                    } else {
                        let op_id = of.operation().read().id();
                        debug_assert!(!e.operations.contains_key(&op_id));
                        e.operations.insert(op_id, of.operation().clone());
                        tokio::spawn(of);
                    }
                }
                e.swap_queues();
            }
        }

        {
            // Engine should be polled until explicitly stopped
            let e = self.read();

            Ok(if e.stopped && e.is_done() {
                Async::Ready(())
            } else {
                /*
                println!("--- engine poll");
                println!("    queue: {}, {}", e.operation_queue1.len(), e.operation_queue2.len());
                println!("    operations: {}", e.operations.len());
                for (id, op) in e.operations.iter() {
                    println!("    {}: {}", id, op.read().label());
                }
                */
                Async::NotReady
            })
        }
    }
}