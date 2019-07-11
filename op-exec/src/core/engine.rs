use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard, Mutex};

use super::*;
use std::sync::mpsc::{sync_channel, Sender};
use std::sync::mpsc::Receiver;
use rayon::{ThreadPool, ThreadPoolBuilder};

#[derive(Debug)]
pub struct Engine {
    config: ConfigRef,
    model_manager: ModelManager,
    exec_manager: ExecManager,
    resource_manager: ResourceManager,
    ssh_session_cache: SshSessionCache,
    stopped: bool,

    progress_receiver: Mutex<Option<Receiver<Progress>>>,
    progress_sender: Mutex<Option<Sender<Progress>>>,
    pool: ThreadPool,
    operation_queue: LinkedHashMap<Uuid, OperationRef>,
    logger: slog::Logger,
}

impl Engine {
    pub fn new(model_dir: PathBuf, config: ConfigRef, logger: slog::Logger) -> Engine {
        let model_manager = ModelManager::new(model_dir,config.clone(), logger.clone());
        let exec_manager = ExecManager::new(config.clone());
        let resource_manager = ResourceManager::new();
        let ssh_session_cache = SshSessionCache::new(config.clone());

        let pool = ThreadPoolBuilder::new()
            .thread_name(|idx| format!("Engine-Worker {}", idx))
//            .num_threads(2)
            .build().unwrap();

        Engine {
            config,
            model_manager,
            exec_manager,
            resource_manager,
            ssh_session_cache,
            stopped: false,

            progress_receiver: Mutex::new(None),
            progress_sender: Mutex::new(None),
            pool,
            operation_queue: LinkedHashMap::new(),
            logger,
        }
    }

    pub fn config(&self) -> &ConfigRef {
        &self.config
    }

    pub fn progress_receiver(&mut self) -> ProgressReceiver {
        let (sender, receiver) = std::sync::mpsc::channel();
        *self.progress_sender.lock().unwrap() = Some(sender);
        receiver.into()
    }

    pub fn notify_progress(&self, progress: Progress){
        let mut receiver_exists = false;
        if let Some(ref s) = *self.progress_sender.lock().unwrap() {
            receiver_exists = s.send(progress).is_ok()
        }

        if !receiver_exists {
            *self.progress_sender.lock().unwrap() = None;
        }
    }

    pub fn schedule_operation(&mut self, op: OperationRef) {
        let uuid = op.read().id();
        self.operation_queue.insert(uuid, op);
    }

    pub fn remove_operation(&mut self, op: &OperationRef) {
        self.operation_queue.remove(&op.read().id());
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

    pub fn resource_manager(&self) -> &ResourceManager {
        &self.resource_manager
    }

    pub fn resource_manager_mut(&mut self) -> &mut ResourceManager {
        &mut self.resource_manager
    }

    pub fn ssh_session_cache(&self) -> &SshSessionCache {
        &self.ssh_session_cache
    }

    pub fn ssh_session_cache_mut(&mut self) -> &mut SshSessionCache {
        &mut self.ssh_session_cache
    }

    pub fn stop(&mut self) {
        self.stopped = true;

        // TODO save queue etc...
        info!(self.logger, "Stopping engine...");

    }
}

#[derive(Debug, Clone)]
pub struct EngineRef(Arc<RwLock<Engine>>);

impl EngineRef {
    pub fn new(model_dir: PathBuf, config: ConfigRef, logger: slog::Logger) -> EngineRef {
        EngineRef(Arc::new(RwLock::new(Engine::new(model_dir, config, logger))))
    }
    pub fn start(current_dir: PathBuf, config: ConfigRef, logger: slog::Logger) -> IoResult<EngineRef> {
        let engine = EngineRef::new(current_dir, config, logger.clone());
        engine.init_model_manager()?;
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

    pub fn init_model_manager(&self) -> IoResult<()> {
        self.write().model_manager.init()
    }

    pub fn init_ssh_session_cache(&self) -> IoResult<()> {
        self.write().ssh_session_cache.init()
    }

    pub fn init_operation_queue(&self) -> IoResult<()> {
//        kg_io::fs::create_dir_all(self.read().config.queue().persist_dir())?;
//        self.load_operation_queue()?;
        Ok(())
    }

//    fn load_operation_queue(&self) -> IoResult<()> {
//        use std::cmp::Ordering;
//        use std::time::UNIX_EPOCH;
//
//        let r = {
//            let engine = self.read();
//            let persist_dir = engine.config.queue().persist_dir();
//            info!(engine.logger, "Loading operation queue from"; o!("path"=>persist_dir.display()));
//
//            kg_io::fs::read_dir(persist_dir)?
//        };
//
//        let mut files: Vec<_> = r
//            .filter_map(|f| f.ok())
//            .map(|f| {
//                let path = f.path();
//                let timestamp = match f.metadata() {
//                    Ok(m) => m.modified().unwrap_or(UNIX_EPOCH),
//                    Err(_) => UNIX_EPOCH,
//                };
//                (path, timestamp)
//            }).collect();
//
//        files.sort_by(|a, b| match a.1.cmp(&b.1) {
//            Ordering::Equal => a.0.cmp(&b.0),
//            o @ _ => o,
//        });
//
//        for (p, _) in files {
//            let s = std::fs::read(p)?;
//            let o: OperationRef = rmp_serde::from_slice(&s).unwrap();
//            self.enqueue_operation(o, false).unwrap();
//        }
//
//        Ok(())
//    }

    pub fn cancel_operation(&self, operation: &OperationRef) {
        operation.write().cancel();
    }

    /// Start operation and wait for result.
    /// DO NOT USE inside operation! It may cause deadlocks.
    pub fn execute_operation(&mut self, operation: OperationRef) -> Result<Outcome, RuntimeError> {
        self.start_operation(operation).receive()
    }

    /// Start nested operation, wait for result and rerun parent operation.
    /// This method should be called only from inside of operations.
    pub fn enqueue_nested_operation(&mut self, operation: OperationRef, parent: OperationRef) {
        eprintln!("enqueue = {:?} ----- parent {}", operation.read().label(), parent.read().label());
        let engine = self.clone();
        parent.write().set_waiting(true);
        engine.write().schedule_operation(operation.clone());
        self.read().pool.spawn(move ||{
            // Execute child operation
            let mut op_impl = create_operation_impl(&operation, &engine);
            let res = op_impl.execute();

            /// this implementation have no sense
            if operation.read().is_waiting(){
                operation.write().set_res_sender(parent.write().res_sender_mut().unwrap().clone());
                return;
            }

            operation.write().set_result(res);
            engine.write().remove_operation(&operation);

            // handle parent
            let mut parent_impl = create_operation_impl(&parent, &engine);
            eprintln!("waking up = {:?} ==== child finished {}", parent.read().label(), operation.read().label());
            match parent_impl.wake_up(operation.clone()) {
                WakeUpStatus::Ready(res) => {
                    // all children finished, send parent result
                    let sender = parent.write().take_res_sender();
                    parent.write().set_waiting(false);
                    engine.write().remove_operation(&parent);
                    eprintln!("Operation ready!= {:?}", parent.read().label());
                    let send_res = sender.send(res);
                    if let Err(_err) = send_res {
                        // receiver dropped
                        info!(engine.read().logger, "Operation result skipped: {}", operation.read().label())
                    }
                },
                WakeUpStatus::NotReady => {
                    eprintln!("Operation not ready... = {:?}", parent.read().label());
                    // parent still waiting for children
                },
            };
        });
    }

    /// Start operation and immediately return result receiver.
    /// DO NOT USE inside operation! It may cause deadlocks.
    pub fn start_operation(&mut self, operation: OperationRef) -> OperationResultReceiver {
        eprintln!(" Start operation = {:?}",operation.read().label());
        let (sender, receiver) = sync_channel(1);
        let engine = self.clone();

        {
            let engine = engine.read();
            let op = operation.read();
            debug!(engine.logger, "New Operation scheduled";
            o!(
                "context"=> format!("{:?}", op.context()),
                "id"=> format!("{}", op.id()),
                 "label"=> format!("{}", op.label()),
              )
            );
        }
        operation.write().set_res_sender(sender.clone());
        engine.write().schedule_operation(operation.clone());
        self.read().pool.spawn(move || {
            let mut op_impl = create_operation_impl(&operation, &engine);
            eprintln!("executing operation= {:?}", operation.read().label());
            let res = op_impl.execute();
            eprintln!("executed operation= {:?}", operation.read().label());

            eprintln!("operation.read().is_waiting() = {:?}", operation.read().is_waiting());
            // Dont send result when operation is waiting for children.
            // Result will be send after children completion
            if !operation.read().is_waiting() || res.is_err(){

                operation.write().res_sender_mut().take();
                engine.write().remove_operation(&operation);
                let send_res = sender.send(res);
                if let Err(_err) = send_res {
                    // receiver dropped
                    info!(engine.read().logger, "Operation result skipped: {}", operation.read().label())
                }
            }

        });
        receiver.into()
    }
}

pub struct OperationResultReceiver(Receiver<Result<Outcome, RuntimeError>>);

impl OperationResultReceiver {

    /// Block and wait for operation result.
    pub fn receive(self) -> Result<Outcome, RuntimeError> {
        self.0.recv().expect("Operation result sender dropped!")
    }
}

impl From<Receiver<Result<Outcome, RuntimeError>>> for OperationResultReceiver {
    fn from(receiver: Receiver<Result<Outcome, RuntimeError>>) -> Self {
        Self(receiver)
    }
}

pub struct ProgressReceiver(Receiver<Progress>);

impl ProgressReceiver {
    /// Block and wait for progress info.
    /// Returns `None` if corresponding sender is dropped
    pub fn receive(&self) -> Option<Progress>{
        self.0.recv().ok()
    }
}

impl From<Receiver<Progress>> for ProgressReceiver {
    fn from(receiver: Receiver<Progress>) -> Self {
        Self(receiver)
    }
}
