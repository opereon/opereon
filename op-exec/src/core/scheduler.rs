use std::collections::VecDeque;
use std::sync::{Arc, Mutex, Condvar, MutexGuard};
use std::thread::JoinHandle;
use std::sync::atomic::{AtomicBool, Ordering};

type Queue<T> = Arc<Mutex<VecDeque<T>>>;

pub trait Task {
    fn execute(&mut self);
    fn id(&self) -> usize {
        0
    }
}

impl <F> Task for F where F: FnMut() {
    fn execute(&mut self) {
        self()
    }
}

pub struct Scheduler<T: 'static + Task + Send> {
    queue: Queue<T>,
    cvar: Arc<Condvar>,
    stopped: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>
}

impl <T: 'static + Task + Send> Scheduler <T> {

    /// Create new Scheduler with provided number of worker threads.
    pub fn new(threads: usize) -> Self{
        let queue = Arc::new(Mutex::new(VecDeque::new()));
        let cvar = Arc::new(Condvar::new());
        let stopped = Arc::new(AtomicBool::new(false));
        let mut workers = Vec::with_capacity(threads);

        for i in 0..threads {
            let mut w = Worker::new(queue.clone(), cvar.clone(), stopped.clone(), i);

            let jh = std::thread::Builder::new()
                .name(format!("Worker-{}", i))
                .spawn(move || w.process()).unwrap();

            workers.push(jh)
        }

        Self {
            queue,
            cvar,
            stopped,
            workers
        }

    }

    /// Stop processing queue, wait for running tasks completion and shutdown workers.
    pub fn stop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.cvar.notify_all();
        self.join();

    }

    /// Schedule new task for execution
    pub fn schedule(&mut self, task: T) {
        let id = task.id();
        self.queue.lock().unwrap().push_back(task);
        self.cvar.notify_one();
    }

    /// Returns locked task queue. Task execution is paused until this lock is released.
    pub fn lookup_queue(&self) -> MutexGuard<VecDeque<T>> {
        self.queue.lock().unwrap()
    }

    fn join(&mut self) {
        for h in self.workers.drain(..) {
            h.join().unwrap();
        }
    }
}

struct Worker<T: 'static + Task + Send> {
    queue: Queue<T>,
    cvar: Arc<Condvar>,
    stopped: Arc<AtomicBool>,
    id: usize,
}

impl<T: 'static + Task + Send> Worker <T> {
    fn new(queue: Queue<T>, cvar: Arc<Condvar>, stopped: Arc<AtomicBool>, id: usize) -> Self{
        Self {
            id,
            queue,
            cvar,
            stopped
        }
    }

    pub fn process(&mut self) {
        let mut task = None;

        loop {
            task = task.or_else(||self.queue.lock().unwrap().pop_front());
            if let Some(mut t) = task.take() {
                t.execute();
            } else {
                let mut guard = self.cvar.wait(self.queue.lock().unwrap()).unwrap();
                // do not schedule task if scheduler stopped
                if self.stopped.load(Ordering::Relaxed) {
                    break
                } else {
                    task = guard.pop_front();
                }
            }
            if self.stopped.load(Ordering::Relaxed) {
                break
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    struct TestTask {
        id: usize
    }

    impl TestTask {
        pub fn new(id: usize) -> Self {
            eprintln!("CREATED = {:?}", id);
            Self {
                id
            }
        }
    }

    impl Task for TestTask {
        fn execute(&mut self) {
//            eprintln!("STARTED = {:?}", self.id);
            std::thread::sleep(Duration::from_secs(2));
//            eprintln!("FINISHED = {:?}", self.id);
        }
        fn id(&self) -> usize {
            self.id
        }
    }

    #[test]
    fn test_resume_after_queue_empty() {
        let mut scheduler = Scheduler::new(2);

        for i in 0..5 {
            scheduler.schedule(TestTask::new(i))
        }

        println!("Waiting for completion....");
        std::thread::sleep(Duration::from_secs(15));

        println!("Scheduling next jobs...");
        for i in 0..5 {
            scheduler.schedule(TestTask::new(i))
        }

        println!("Waiting....");
        std::thread::sleep(Duration::from_secs(15));
        println!("Stopping scheduler....");

        scheduler.stop();


    }

    #[test]
    fn test_closures() {
        let mut scheduler = Scheduler::new(12);

        for i in 0..20 {
            scheduler.schedule(move ||{
                eprintln!("STARTED = {:?}", i);
                std::thread::sleep(Duration::from_secs(2));
                eprintln!("FINISHED = {:?}", i);
            })
        }

        println!("Waiting for completion....");
        std::thread::sleep(Duration::from_secs(30));
        println!("Stopping scheduler....");

        scheduler.stop();
    }

    #[test]
    fn test_all_tasks_executed() {
        let mut scheduler = Scheduler::new(12);
        let mut results = vec![];

        for i in 0..100 {
            let (s, r) = std::sync::mpsc::channel();
            scheduler.schedule(move ||{
                std::thread::sleep(Duration::from_millis(20));
                s.send(()).unwrap();
            });
            results.push(r);
        }

        for r in results {
            r.recv_timeout(Duration::from_secs(15)).unwrap();
        }
        scheduler.stop();
    }
}