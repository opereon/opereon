use super::*;


#[derive(Serialize, Deserialize)]
struct AsPath {
    path: String
}

pub struct RunExec {
    #[serde(serialize_with = "RunExec::store_steps", deserialize_with = "RunExec::load_steps")]
    steps: Vec<StepExec>,
}

impl RunExec {
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
}