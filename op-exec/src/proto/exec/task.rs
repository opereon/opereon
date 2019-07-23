use super::*;

#[derive(Debug, Display, PartialEq, Eq, Serialize, Deserialize)]
#[display(fmt = "{name}")]
pub struct TaskExec {
    name: String,
    kind: TaskKind,
    task_path: Opath,
}

impl TaskExec {
    pub fn create(
        _model: &Model,
        _proc: &ProcDef,
        task: &TaskDef,
        _host: &HostDef,
        _proc_exec: &ProcExec,
        _step_exec: &StepExec,
    ) -> Result<TaskExec, ProtoError> {
        Ok(TaskExec {
            name: task.label().to_string(),
            kind: task.kind(),
            task_path: Opath::from(task.node()),
        })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> TaskKind {
        self.kind
    }

    pub fn task_path(&self) -> &Opath {
        &self.task_path
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskResult {
    outcome: Outcome,
    status: Option<i32>,
    signal: Option<i32>,
}

impl TaskResult {
    pub fn new(outcome: Outcome, status: Option<i32>, signal: Option<i32>) -> TaskResult {
        TaskResult {
            outcome,
            status,
            signal,
        }
    }

    pub fn is_success(&self) -> bool {
        if let Some(status) = self.status {
            status == 0
        } else {
            false
        }
    }

    pub fn is_error(&self) -> bool {
        if let Some(status) = self.status {
            status != 0
        } else {
            false
        }
    }

    pub fn is_interrupted(&self) -> bool {
        self.status.is_none()
    }

    pub fn status(&self) -> Option<i32> {
        self.status
    }

    pub fn signal(&self) -> Option<i32> {
        self.signal
    }

    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    pub fn into_outcome(self) -> Outcome {
        self.outcome
    }
}

impl std::fmt::Display for TaskResult {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(code) = self.status() {
            write!(f, "status: {}", code)?
        } else {
            write!(f, "status: ?")?
        }
        if let Some(signal) = self.signal() {
            write!(f, ", signal: {}", signal)?
        }
        write!(f, ", {}", self.outcome)
    }
}
