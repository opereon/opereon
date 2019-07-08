use super::*;

#[derive(Serialize, Deserialize)]
struct AsPath {
    path: String
}


#[derive(Debug, Serialize, Deserialize)]
pub struct RunExec {
    #[serde(serialize_with = "RunExec::store_steps", deserialize_with = "RunExec::load_steps")]
    steps: Vec<StepExec>,
}

impl RunExec {
    pub fn new() -> RunExec {
        RunExec {
            steps: Vec::new(),
        }
    }

    fn store_steps<S>(steps: &Vec<StepExec>, serializer: S) -> Result<S::Ok, S::Error> where S: serde::Serializer {
        serializer.collect_seq(steps.iter().map(|e| {
            let dir = e.path().file_name().unwrap();
            let mut p = PathBuf::new();
            p.push(dir);
            p.push("_step.yaml");
            AsPath { path: p.to_str().unwrap().into() }
        }))
    }

    fn load_steps<'de, D>(deserializer: D) -> Result<Vec<StepExec>, D::Error> where D: serde::Deserializer<'de> {
        use serde::Deserialize;

        let paths: Vec<AsPath> = Vec::deserialize(deserializer)?;

        let steps = EXEC_PATH.with(|exec_path| {
            let exec_path = exec_path.borrow();

            let mut steps = Vec::with_capacity(paths.len());
            for path in paths {
                let path = exec_path.join(&path.path);
                let s = fs::read_string(&path).unwrap();
                let mut step: StepExec = serde_yaml::from_str(&s).unwrap();
                step.set_path(path.parent().unwrap().into());
                steps.push(step);
            }
            steps
        });

        Ok(steps)
    }

    pub fn steps(&self) -> &[StepExec] {
        &self.steps
    }

    pub fn add_step(&mut self, step: StepExec) {
        self.steps.push(step);
    }
}