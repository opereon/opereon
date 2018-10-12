use super::*;


#[derive(Debug, Serialize, Deserialize)]
pub struct Job {
    host_path: Opath,
    step: usize,
    host: Host,
    actions: Vec<Action>,
    #[serde(skip)]
    path: PathBuf,
}

impl Job {
    pub fn create(model: &Model, procedure: &ProcDef, step: &Step, host: &HostDef, work: &Work) -> Result<Job, ProtoError> {
        let mut job = Job {
            host_path: host.node().path(),
            step: step.index(),
            host: Host::from_def(host)?,
            actions: Vec::new(),
            path: PathBuf::new(),
        };

        job.prepare(work)?;

        for task in step.tasks().iter() {
            let a = Action::create(model, procedure, task, host, work, &job)?;
            job.add_action(a);
        }

        Ok(job)
    }

    fn get_job_dir_name(&self) -> String {
        format!("{:03}_{}", self.step + 1, self.host.hostname())
    }

    fn create_job_dir(&mut self, work_dir: &Path) -> Result<(), ProtoError> {
        let p = work_dir.join(self.get_job_dir_name());
        debug_assert!(!p.exists());
        std::fs::create_dir(&p)?;
        self.path = p;
        Ok(())
    }

    pub fn prepare(&mut self, work: &Work) -> Result<(), ProtoError> {
        self.create_job_dir(work.path())
    }

    pub fn store(&self) -> Result<(), ProtoError> {
        let e = serde_yaml::to_string(self).unwrap();
        let p = self.path.join("_job.yaml");
        debug_assert!(!p.exists());
        std::fs::write(p, e.as_bytes())?;
        Ok(())
    }

    pub fn add_action(&mut self, action: Action) {
        self.actions.push(action);
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn set_path(&mut self, path: PathBuf) {
        self.path = path;
    }

    pub fn step(&self) -> usize {
        self.step
    }

    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    pub fn host(&self) -> &Host {
        &self.host
    }

    pub fn host_path(&self) -> &Opath {
        &self.host_path
    }
}

impl PartialEq for Job {
    fn eq(&self, other: &Self) -> bool {
        self.path.eq(&other.path)
    }
}

impl Eq for Job {}

