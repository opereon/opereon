use super::*;

#[derive(Debug, Serialize, Deserialize)]
pub struct StepExec {
    step: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    host_path: Option<Opath>,
    host: Host,
    tasks: Vec<TaskExec>,
    #[serde(skip)]
    path: PathBuf,
}

impl StepExec {
    pub fn create(
        model: &Model,
        proc: &ProcDef,
        step: &Step,
        host: &HostDef,
        proc_exec: &ProcExec,
    ) -> ProtoResult<StepExec> {
        let mut step_exec = StepExec {
            step: step.index(),
            // only set host_path if host is defined inside the model
            host_path: model.get_host(host.node()).map(|h| h.node().path()),
            host: Host::from_def(model, host)?,
            tasks: Vec::new(),
            path: PathBuf::new(),
        };

        step_exec.prepare(proc_exec)?;

        for task in step.tasks().iter() {
            let t = TaskExec::create(model, proc, task, host, proc_exec, &step_exec);
            step_exec.add_task(t);
        }

        Ok(step_exec)
    }

    fn get_step_dir_name(&self) -> String {
        format!("{:03}_{}", self.step + 1, self.host.hostname())
    }

    fn create_step_dir(&mut self, proc_exec_dir: &Path) -> ProtoResult<()> {
        let p = proc_exec_dir.join(self.get_step_dir_name());
        fs::create_dir(&p)?;
        self.path = p;
        Ok(())
    }

    pub fn prepare(&mut self, proc_exec: &ProcExec) -> ProtoResult<()> {
        self.create_step_dir(proc_exec.path())
    }

    pub fn store(&self) -> ProtoResult<()> {
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

    pub fn host_path(&self) -> Option<&Opath> {
        self.host_path.as_ref()
    }
}

impl PartialEq for StepExec {
    fn eq(&self, other: &Self) -> bool {
        self.path.eq(&other.path)
    }
}

impl Eq for StepExec {}
