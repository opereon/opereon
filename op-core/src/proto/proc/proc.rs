use super::*;

use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcExec {
    created: DateTime<Utc>,
    name: String,
    label: String,
    kind: ProcKind,
    curr_model: RevInfo,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    prev_model: Option<RevInfo>,
    proc_path: Opath,
    run: RunExec,
    #[serde(skip)]
    path: PathBuf,
}

impl ProcExec {
    pub fn new(created: DateTime<Utc>) -> ProcExec {
        ProcExec {
            created,
            name: String::new(),
            label: String::new(),
            kind: ProcKind::default(),
            curr_model: RevInfo::default(),
            prev_model: None,
            proc_path: Opath::null(),
            run: RunExec::new(),
            path: PathBuf::new(),
        }
    }


    fn get_proc_exec_dir_name(&self) -> String {
        let mut dir_name = format!("{}_{}", self.created.format("%Y%m%d_%H%M%S%.3f"), self.name);
        unsafe {
            dir_name.as_mut_vec()[15] = b'_';
        }
        dir_name
    }

    #[instrument(skip(self))]
    fn create_proc_exec_dir(&mut self, proc_exec_dir: &Path) -> ProtoResult<()> {
        let p = proc_exec_dir.join(self.get_proc_exec_dir_name());
        debug!("Creating procedure execution dir");
        debug_assert!(!p.exists());
        fs::create_dir_all(&p)
            .into_diag_res()
            .map_err_as_cause(|| ProtoErrorDetail::ProcExecDir)?;
        self.path = p;
        Ok(())
    }

    pub fn prepare(
        &mut self,
        model: &Model,
        proc: &ProcDef,
        proc_exec_dir: &Path,
    ) -> ProtoResult<()> {
        self.name = proc.id().to_string();
        self.label = proc.label().to_string();
        self.kind = proc.kind();

        self.curr_model = model.rev_info().clone();

        self.proc_path = proc.node().path();

        let scope = proc.scope_mut()?;

        if proc_exec_dir.is_absolute() {
            self.create_proc_exec_dir(proc_exec_dir)?;
        } else {
            self.create_proc_exec_dir(&model.rev_info().path().join(proc_exec_dir))?;
        }
        for s in proc.run().steps().iter() {
            let hosts = s.resolve_hosts(model, proc)?;
            if hosts.is_empty() {
                warn!(verb=0, "Procedure '{name}', label: '{label}' will not be executed, there is no target hosts!", name=&self.name, label=&self.label);
            }
            for host in hosts {
                let step = StepExec::create(model, proc, s, &host, self)
                    .map_err_as_cause(|| ProtoErrorDetail::StepExecCreate)?;
                self.add_step(step);
            }
        }

        Ok(())
    }

    pub fn add_step(&mut self, step: StepExec) {
        self.run.add_step(step);
    }

    pub fn set_prev_model(&mut self, prev_model: Option<RevInfo>) {
        self.prev_model = prev_model;
    }

    pub fn store(&self) -> ProtoResult<()> {
        for s in self.run.steps().iter() {
            s.store()?;
        }
        let w = serde_yaml::to_string(self).unwrap();
        let p = self.path.join("_proc.yaml");
        debug_assert!(!p.exists());
        fs::write(p, w.as_bytes())?;
        Ok(())
    }

    pub fn load<P: AsRef<Path>>(dir: P) -> ProtoResult<ProcExec> {
        let dir = fs::canonicalize(dir.as_ref())?;

        EXEC_PATH.with(|path| {
            *path.borrow_mut() = dir.clone();
        });

        let file_path = dir.join("_proc.yaml");
        let s = fs::read_string(&file_path)?;
        //FIXME (ws) use kg_tree::serial::yaml
        let mut p: ProcExec = serde_yaml::from_str(&s)
            .map_err(|err| -> BasicDiag {
                println!("error: {}", err);
                unimplemented!()
            })
            .map_err_as_cause(|| ProtoErrorDetail::ProcExecLoad {
                file_path: file_path.to_string_lossy().to_string(),
            })?;

        p.path = dir;
        Ok(p)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn kind(&self) -> ProcKind {
        self.kind
    }

    pub fn curr_model(&self) -> &RevInfo {
        &self.curr_model
    }

    pub fn prev_model(&self) -> Option<&RevInfo> {
        self.prev_model.as_ref()
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

    pub fn run(&self) -> &RunExec {
        &self.run
    }
}

impl PartialEq for ProcExec {
    fn eq(&self, other: &Self) -> bool {
        self.path.eq(&other.path)
    }
}

impl Eq for ProcExec {}

#[derive(Debug, Clone)]
pub struct ProcExecRef(Arc<Mutex<ProcExec>>);

impl ProcExecRef {
    pub fn new(proc: ProcExec) -> ProcExecRef {
        ProcExecRef(Arc::new(Mutex::new(proc)))
    }

    pub fn lock(&self) -> MutexGuard<ProcExec> {
        self.0.lock().unwrap()
    }
}

impl PartialEq for ProcExecRef {
    fn eq(&self, other: &ProcExecRef) -> bool {
        if Arc::ptr_eq(&self.0, &other.0) {
            true
        } else {
            *self.lock() == *other.lock()
        }
    }
}

impl Eq for ProcExecRef {}

unsafe impl Send for ProcExecRef {}

unsafe impl Sync for ProcExecRef {}
