use super::*;
use std::process::Stdio;

static COMPOSE_CMD: &str = "docker-compose";

pub struct Context {
    tmp: TempDir,
    tmp_dir: PathBuf,
    compose_file: PathBuf,
    model_dir: PathBuf,
}

impl Context {
    pub fn new() -> Context {
        let (tmp, dir) = get_tmp_dir();
        let compose = dir.join("docker-compose.yml");
        let model = dir.join("model");
        copy_resource!("compose/docker-compose.yml", compose);
        copy_resource!("compose/ares", dir.join("ares"));
        copy_resource!("compose/zeus", dir.join("zeus"));

        copy_resource!("model", model);
        copy_resource!("keys", dir.join("keys"));

        init_repo(&model);
        initial_commit(&model);

        let out = Command::new(COMPOSE_CMD)
            .args(&["-f", &compose.to_string_lossy()])
            .args(&["up", "-d"])
            .output()
            .unwrap();

        if !out.status.success() {
            eprintln!("compose up status = {:?}", out);
            panic!()
        }

        Context {
            tmp,
            compose_file: compose,
            model_dir: model,
            tmp_dir: dir,
        }
    }

    pub fn exec_op(&self, args: &[&str]) -> (String, String, i32) {
        let out = Command::new("op")
            .args(args)
            .stdin(Stdio::inherit())
            .current_dir(&self.model_dir)
            .output()
            .unwrap();

        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let code = out.status.code().expect("Process terminated");
            (stdout, stderr, code)

        } else {
            panic!("error calling op command: {:?}", out)
        }
    }

    pub fn collect_logs(&mut self) -> String {
        let logs = Command::new(COMPOSE_CMD)
            .args(&["-f", &self.compose_file.to_string_lossy()])
            .arg("logs")
            .output()
            .unwrap()
            .stdout;

        String::from_utf8_lossy(&logs).to_string()
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    pub fn tmp_dir(&self) -> &Path {
        &self.tmp_dir
    }
}

impl std::ops::Drop for Context {
    fn drop(&mut self) {
        let res = Command::new(COMPOSE_CMD)
            .args(&["-f", &self.compose_file.to_string_lossy()])
            .args(&["down", "-v"])
            .output();

        match res {
            Ok(out) => {
                if !out.status.success() {
                    eprintln!("error stopping containers = {:?}", out);
                    let logs = self.collect_logs();
                    eprintln!("logs = {}", logs);
                }
            }
            Err(err) => {
                eprintln!("err = {:?}", err);
                let logs = self.collect_logs();
                eprintln!("logs = {}", logs);
            }
        }
    }
}
