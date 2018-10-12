use super::*;

use std::sync::{Arc, Mutex, MutexGuard};
use std::cell::RefCell;

thread_local!(static WORK_PATH: RefCell<PathBuf> = RefCell::new(PathBuf::new()));

#[derive(Serialize, Deserialize)]
struct AsPath {
    path: String
}


#[derive(Debug, Serialize, Deserialize)]
pub struct Work {
    name: String,
    model_path: ModelPath,
    proc_path: Opath,
    args: Arguments,
    created: DateTime<Utc>,
    #[serde(serialize_with = "Work::store_jobs", deserialize_with = "Work::load_jobs")]
    jobs: Vec<Job>,
    #[serde(skip)]
    path: PathBuf,
}

impl Work {
    pub fn new(created: DateTime<Utc>) -> Work {
        Work {
            name: String::new(),
            model_path: ModelPath::Current,
            proc_path: Opath::null(),
            args: Arguments::new(),
            created,
            path: PathBuf::new(),
            jobs: Vec::new(),
        }
    }

    pub fn with_args(created: DateTime<Utc>, args: Arguments) -> Work {
        Work {
            name: String::new(),
            model_path: ModelPath::Current,
            proc_path: Opath::null(),
            args,
            created,
            path: PathBuf::new(),
            jobs: Vec::new(),
        }
    }

    fn get_work_dir_name(&self) -> String {
        let mut dir_name = self.created.format("%Y%m%d_%H%M%S%.3f").to_string();
        unsafe { dir_name.as_mut_vec()[15] = b'_'; }
        dir_name
    }

    pub fn prepare(&mut self, model: &Model, proc: &ProcDef, work_dir: &Path) -> Result<(), ProtoError> {
        if work_dir.is_absolute() {
            self.create_work_dir(work_dir)?;
        } else {
            self.create_work_dir(&model.metadata().path().join(work_dir))?;
        }

        self.name = proc.label().to_string();

        self.model_path = if model.metadata().is_stored() {
            ModelPath::Id(model.metadata().id())
        } else {
            ModelPath::Path(model.metadata().path().to_path_buf())
        };

        self.proc_path = proc.node().path();

        self.args.resolve(proc.root(), proc.node(), proc.scope_mut());

        for s in proc.run().steps().iter() {
            let hosts = s.resolve_hosts(model, proc)?;

            for host in hosts {
                let job = Job::create(model, proc, s, host, self)?;
                self.add_job(job);
            }
        }

        Ok(())
    }

    fn create_work_dir(&mut self, work_dir: &Path) -> Result<(), ProtoError> {
        let p = work_dir.join(self.get_work_dir_name());
        debug_assert!(!p.exists());
        std::fs::create_dir_all(&p)?;
        self.path = p;
        Ok(())
    }

    pub fn add_job(&mut self, job: Job) {
        self.jobs.push(job);
    }

    pub fn store(&self) -> Result<(), ProtoError> {
        for e in self.jobs.iter() {
            e.store()?;
        }
        let w = serde_yaml::to_string(self).unwrap();
        let p = self.path.join("_work.yaml");
        debug_assert!(!p.exists());
        std::fs::write(p, w.as_bytes())?;
        Ok(())
    }

    fn store_jobs<S>(jobs: &Vec<Job>, serializer: S) -> Result<S::Ok, S::Error> where S: serde::Serializer {
        serializer.collect_seq(jobs.iter().map(|e| {
            let dir = e.path().file_name().unwrap();
            let mut p = PathBuf::new();
            p.push(dir);
            p.push("_job.yaml");
            AsPath { path: p.to_str().unwrap().into() }
        }))
    }

    //FIXME (jc) errors
    pub fn load<P: AsRef<Path>>(dir: P) -> Result<Work, ProtoError> {
        let dir = dir.as_ref().canonicalize()?;

        WORK_PATH.with(|path| {
            *path.borrow_mut() = dir.clone();
        });

        let s = String::from_utf8(std::fs::read(dir.join("_work.yaml"))?).unwrap(); //FIXME (jc)
        let mut w: Work = serde_yaml::from_str(&s).unwrap(); //FIXME (jc)

        w.path = dir;
        Ok(w)
    }

    //FIXME (jc) errors
    fn load_jobs<'de, D>(deserializer: D) -> Result<Vec<Job>, D::Error> where D: serde::Deserializer<'de> {
        use serde::Deserialize;

        let paths: Vec<AsPath> = Vec::deserialize(deserializer)?;

        let jobs = WORK_PATH.with(|work_path| {
            let work_path = work_path.borrow();

            let mut jobs = Vec::with_capacity(paths.len());
            for path in paths {
                let path = work_path.join(&path.path);
                let s = String::from_utf8(std::fs::read(&path).unwrap()).unwrap();
                let mut job: Job = serde_yaml::from_str(&s).unwrap();
                job.set_path(path.parent().unwrap().into());
                jobs.push(job);
            }
            jobs
        });

        Ok(jobs)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn model_path(&self) -> &ModelPath {
        &self.model_path
    }

    pub fn proc_path(&self) -> &Opath {
        &self.proc_path
    }

    pub fn created(&self) -> DateTime<Utc> {
        self.created
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn jobs(&self) -> &[Job] {
        &self.jobs
    }

    pub fn args(&self)-> &Arguments{
        &self.args
    }
}

impl PartialEq for Work {
    fn eq(&self, other: &Self) -> bool {
        self.path.eq(&other.path)
    }
}

impl Eq for Work {}


#[derive(Debug, Clone)]
pub struct WorkRef(Arc<Mutex<Work>>);

impl WorkRef {
    pub fn new(work: Work) -> WorkRef {
        WorkRef(Arc::new(Mutex::new(work)))
    }

    pub fn lock(&self) -> MutexGuard<Work> {
        self.0.lock().unwrap()
    }
}

impl PartialEq for WorkRef {
    fn eq(&self, other: &WorkRef) -> bool {
        if Arc::ptr_eq(&self.0, &other.0) {
            true
        } else {
            *self.lock() == *other.lock()
        }
    }
}

impl Eq for WorkRef {}

unsafe impl Send for WorkRef {}

unsafe impl Sync for WorkRef {}
