use super::*;
use std::process::Stdio;

static COMPOSE_CMD: &str = "docker-compose";

pub struct Context {
    tmp: TempDir,
    tmp_dir: PathBuf,
    compose_file: PathBuf,
    model_dir: PathBuf,
}

//fn check_host(port: &str) -> Result<(), ()> {
//    let status = Command::new("ssh")
//        .args(&["root@127.0.0.1", "-p", port])
//        .args(&["echo", "works"])
//        .spawn()
//        .map_err(|_| ())?
//        .wait()
//        .map_err(|_|())?;
//    if status.success() {
//        Ok(())
//    } else {
//        Err(())
//    }
//}

impl Context {
    pub fn new() -> Context {
        let (tmp, dir) = get_tmp_dir();
        let compose = dir.join("docker-compose.yml");
        let model = dir.join("model");
        copy_resource!("compose/docker-compose.yml", compose);
        copy_resource!("compose/ares", dir.join("ares"));
        copy_resource!("compose/zeus", dir.join("zeus"));

        copy_resource!("model", model);

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

        let ctx = Context {
            tmp,
            compose_file: compose,
            model_dir: model,
            tmp_dir: dir,
        };
        ctx.wait_for_ssh_up();
        ctx
    }

    pub fn exec_op(&self, args: &[&str]) -> (String, String, i32) {
        let out = Command::new("op")
            .args(args)
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

    pub fn wait_for_ssh_up(&self) {
        std::thread::sleep(Duration::from_secs(2))

        //        let mut zeus = check_host("8820");
        //        let mut ares = check_host("8820");
        //
        //        for _i in 0..10 {
        //            println!("attempt to connect...");
        //            match (zeus.is_ok(), ares.is_ok()) {
        //                (true, true) => return,
        //                (false, true) => zeus = check_host("8820"),
        //                (true, false) => ares = check_host("8821"),
        //                (false, false) => {
        //                    zeus = check_host("8820");
        //                    ares = check_host("8821");
        //                }
        //            }
        //            println!("hosts not ready...");
        //            std::thread::sleep(Duration::from_millis(100))
        //        }
        //        panic!("Cannot establish ssh connections!")
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

macro_rules! strip_ansi {
    ($text: expr) => {{
        console::strip_ansi_codes(&$text).to_string()
    }};
}
