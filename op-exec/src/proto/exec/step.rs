use super::*;


#[derive(Debug, Serialize, Deserialize)]
pub struct StepExec {
    step: usize,
    host_path: Opath,
    host: Host,
    tasks: Vec<TaskExec>,
    #[serde(skip)]
    path: PathBuf,
}

impl StepExec {
    pub fn create(model: &Model, proc: &ProcDef, step: &Step, host: &HostDef, proc_exec: &ProcExec) -> Result<StepExec, ProtoError> {
        let mut step_exec = StepExec {
            step: step.index(),
            host_path: host.node().path(),
            host: Host::from_def(host)?,
            tasks: Vec::new(),
            path: PathBuf::new(),
        };

        step_exec.prepare(proc_exec)?;

        for task in step.tasks().iter() {
            let t = TaskExec::create(model, proc, task, host, proc_exec, &step_exec)?;
            step_exec.add_task(t);
        }

        Ok(step_exec)
    }

    fn get_step_dir_name(&self) -> String {
        format!("{:03}_{}", self.step + 1, self.host.hostname())
    }

    fn create_step_dir(&mut self, proc_exec_dir: &Path) -> Result<(), ProtoError> {
        let p = proc_exec_dir.join(self.get_step_dir_name());
        debug_assert!(!p.exists());
        fs::create_dir(&p)?;
        self.path = p;
        Ok(())
    }

    pub fn prepare(&mut self, proc_exec: &ProcExec) -> Result<(), ProtoError> {
        self.create_step_dir(proc_exec.path())
    }

    pub fn store(&self) -> Result<(), ProtoError> {
        let e = serde_yaml::to_string(self).unwrap();
        let p = self.path.join("_step.yaml");
        debug_assert!(!p.exists());
        fs::write(p, e.as_bytes())?;
        Ok(())
    }

    pub fn add_task(&mut self, task: TaskExec) {
        self.tasks.push(task);
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

    pub fn tasks(&self) -> &[TaskExec] {
        &self.tasks
    }

    pub fn host(&self) -> &Host {
        &self.host
    }

    pub fn host_path(&self) -> &Opath {
        &self.host_path
    }
}

impl PartialEq for StepExec {
    fn eq(&self, other: &Self) -> bool {
        self.path.eq(&other.path)
    }
}

impl Eq for StepExec {}
