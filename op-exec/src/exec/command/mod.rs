use std::io::{Cursor, SeekFrom};
use std::process::{Child, Command};
use std::thread::JoinHandle;

use rand::Rng;
use regex::Regex;

use super::*;

pub use self::config::*;
pub use self::local::*;
pub use self::ssh::*;
mod config;
mod local;
mod ssh;

//FIXME (jc)
#[derive(Debug, Clone)]
pub enum CommandError {
    Undef,
}

impl From<SshError> for CommandError {
    fn from(err: SshError) -> Self {
        eprintln!("ssh error: {:?}", err);
        CommandError::Undef
    }
}

impl From<std::io::Error> for CommandError {
    fn from(err: std::io::Error) -> Self {
        eprintln!("io error: {:?}", err);
        CommandError::Undef
    }
}

impl From<std::fmt::Error> for CommandError {
    fn from(err: std::fmt::Error) -> Self {
        eprintln!("fmt error: {:?}", err);
        CommandError::Undef
    }
}

impl From<kg_diag::IoError> for CommandError {
    fn from(err: kg_diag::IoError) -> Self {
        eprintln!("io error: {:?}", err);
        CommandError::Undef
    }
}

impl From<kg_tree::TreeErrorDetail> for CommandError {
    fn from(err: kg_tree::TreeErrorDetail) -> Self {
        eprintln!("tree error: {:?}", err);
        CommandError::Undef
    }
}

pub type EnvVars = LinkedHashMap<String, String>;

pub enum SourceRef<'a> {
    Path(&'a Path),
    Source(&'a str),
}

impl<'a> SourceRef<'a> {
    pub fn read(&self) -> Result<String, kg_diag::IoError> {
        match *self {
            SourceRef::Path(path) => {
                let mut s = String::new();
                fs::read_to_string(path, &mut s)?;
                Ok(s)
            }
            SourceRef::Source(src) => Ok(src.into()),
        }
    }

    pub fn read_to(&self, buf: &mut String) -> Result<(), kg_diag::IoError> {
        match *self {
            SourceRef::Path(path) => {
                fs::read_to_string(path, buf)?;
                Ok(())
            }
            SourceRef::Source(src) => {
                buf.push_str(src);
                Ok(())
            }
        }
    }
}

pub trait CommandExecutor {
    fn exec_command(
        &mut self,
        engine: &EngineRef,
        cmd: &str,
        args: &[String],
        out_format: Option<FileFormat>,
        log: &OutputLog,
    ) -> Result<TaskResult, CommandError>;

    fn exec_script(
        &mut self,
        engine: &EngineRef,
        script: SourceRef,
        args: &[String],
        env: Option<&EnvVars>,
        cwd: Option<&Path>,
        run_as: Option<&str>,
        out_format: Option<FileFormat>,
        log: &OutputLog,
    ) -> Result<TaskResult, CommandError>;
}

pub fn create_command_executor(
    host: &Host,
    engine: &EngineRef,
) -> Result<Box<dyn CommandExecutor>, CommandError> {
    let e = engine
        .write()
        .ssh_session_cache_mut()
        .get(host.ssh_dest())?;
    Ok(Box::new(e))
}

pub fn resolve_env(
    env: &TaskEnv,
    root: &NodeRef,
    current: &NodeRef,
    scope: &Scope,
) -> RuntimeResult<EnvVars> {
    lazy_static! {
        static ref VAR_NAME_RE: Regex = Regex::new(r"[^A-Za-z0-9]").unwrap();
    };

    fn env_name_from_path(node: &NodeRef) -> Option<String> {
        let mut env_name = vec![];
        let mut current = Some(node.clone());

        while let Some(c) = current {
            let node = c.data();
            let key = node.key();
            if key.is_empty() && !node.is_root() {
                eprintln!("Warn! Cannot create env var from node, empty key.  {}", c);
                return None;
            }

            env_name.insert(0, key.to_string());

            current = node.parent()
        }

        let env_name = env_name.as_slice().join(" ").to_uppercase();
        let env_name = VAR_NAME_RE.replace_all(env_name.trim(), "_");

        Some(env_name.to_string())
    }

    match env {
        TaskEnv::List(items) => {
            let mut resolved = LinkedHashMap::with_capacity(items.len());

            for expr in items.iter() {
                let res = expr.apply_one_ext(root, current, scope)?;
                if res.is_string() {
                    if let Some(env_name) = env_name_from_path(&res) {
                        let prev = resolved.insert(env_name.clone(), res.as_string());
                        if prev.is_some() {
                            eprintln!("Warn! Duplicated env variable name = {:?}", env_name);
                        }
                    }
                } else {
                    eprintln!(
                        "Warn! Cannot create env var from node: {}. String expected ",
                        res
                    );
                }
            }

            Ok(resolved)
        }
        TaskEnv::Map(items) => {
            let mut resolved = LinkedHashMap::with_capacity(items.len());

            for (name, expr) in items.iter() {
                let res = expr.apply_one_ext(root, current, scope)?;
                if res.is_string() {
                    resolved.insert(name.clone(), res.as_string());
                } else {
                    eprintln!(
                        "Warn! Cannot create env variable from node: String expected {}",
                        res
                    );
                }
            }

            Ok(resolved)
        }
    }
}

fn prepare_script<W: Write>(
    script: SourceRef,
    args: &[String],
    env: Option<&EnvVars>,
    cwd: Option<&Path>,
    mut out: W,
) -> Result<(), IoError> {
    let mut rng = rand::thread_rng();

    let script = script.read()?;

    write!(out, "#!/usr/bin/env bash\n")?;

    if let Some(cwd) = cwd {
        write!(out, "cd \"{}\"\n", cwd.display())?;
    }
    if let Some(env) = env {
        for (k, v) in env {
            write!(out, "export {}='{}'\n", k, v)?;
        }
    }

    // Create temp script file in ramdisk
    let tmp_path = format!("/dev/shm/op_{:0x}", rng.gen::<u64>());
    write!(out, "cat > {} <<-'%%EOF%%'\n", tmp_path)?;
    write!(out, "{}\n", script.trim())?;
    write!(out, "%%EOF%%\n")?;

    // Make temp script executable
    write!(out, "chmod +x {}\n", tmp_path)?;

    // Execute tmp script
    if args.is_empty() {
        write!(out, "({})\n", tmp_path)?;
    } else {
        write!(out, "({}", tmp_path)?;
        for arg in args {
            write!(out, " \'{}\'", arg)?;
        }
        write!(out, ")\n")?;
    }

    // Capture script status
    write!(out, "STATUS=$?\n")?;

    // Remove temp script
    write!(out, "rm -f {}\n", tmp_path)?;

    // Exit with tmp script status code
    write!(out, "exit $STATUS\n")?;

    Ok(())
}

fn execute(
    mut child: Child,
    out_format: Option<FileFormat>,
    err_format: Option<FileFormat>,
    log: &OutputLog,
) -> Result<TaskResult, CommandError> {
    use std::io::BufRead;

    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let log_err = log.clone();
    let log_out = log.clone();

    let herr: JoinHandle<std::io::Result<_>> = if err_format.is_some() {
        std::thread::spawn(move || {
            let mut stderr_buf: Cursor<Vec<u8>> = Cursor::new(Vec::new());
            {
                let mut r = BufReader::new(&mut stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    let len = r.read_line(&mut line)?;
                    if len == 0 {
                        break;
                    } else {
                        log_err.log_stderr(line.as_bytes())?;
                        stderr_buf.write_all(line.as_bytes())?;
                    }
                }
            }
            Ok(stderr_buf)
        })
    } else {
        std::thread::spawn(move || {
            let mut r = BufReader::new(&mut stderr);
            let mut line = String::new();
            loop {
                line.clear();
                let len = r.read_line(&mut line)?;
                if len == 0 {
                    break;
                } else {
                    log_err.log_stderr(line.as_bytes())?;
                }
            }
            Ok(Cursor::new(Vec::new()))
        })
    };

    let hout: JoinHandle<std::io::Result<_>> = if out_format.is_some() {
        std::thread::spawn(move || {
            let mut stdout_buf: Cursor<Vec<u8>> = Cursor::new(Vec::new());
            {
                let mut r = BufReader::new(&mut stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    let len = r.read_line(&mut line)?;
                    if len == 0 {
                        break;
                    } else {
                        log_out.log_stdout(line.as_bytes())?;
                        stdout_buf.write_all(line.as_bytes())?;
                    }
                }
            }
            Ok(stdout_buf)
        })
    } else {
        std::thread::spawn(move || {
            let mut r = BufReader::new(&mut stdout);
            let mut line = String::new();
            loop {
                line.clear();
                let len = r.read_line(&mut line)?;
                if len == 0 {
                    break;
                } else {
                    log_out.log_stdout(line.as_bytes())?;
                }
            }
            Ok(Cursor::new(Vec::new()))
        })
    };

    let status = child.wait()?;
    let mut stdout = hout.join().unwrap()?;
    let mut stderr = herr.join().unwrap()?;
    stdout.seek(SeekFrom::Start(0))?;
    stderr.seek(SeekFrom::Start(0))?;

    log.log_status(status.code())?;

    let outcome = if let Some(fmt) = out_format {
        // FIXME ws error handling
        let n = NodeRef::from_bytes(stdout.get_ref(), fmt).expect("cannot build node from bytes!");
        Outcome::NodeSet(n.into())
    } else {
        Outcome::Empty
    };

    Ok(TaskResult::new(outcome, status.code(), None))
}

#[derive(Debug, Clone)]
pub struct CommandBuilder {
    cmd: String,
    args: Vec<String>,
    envs: LinkedHashMap<String, String>,
    setsid: bool,
}

impl CommandBuilder {
    pub fn new<S: Into<String>>(cmd: S) -> CommandBuilder {
        CommandBuilder {
            cmd: cmd.into(),
            args: Vec::new(),
            envs: LinkedHashMap::new(),
            setsid: false,
        }
    }

    pub fn arg<S: Into<String>>(&mut self, arg: S) -> &mut CommandBuilder {
        self.args.push(arg.into());
        self
    }

    pub fn args<S: Into<String>, I: Iterator<Item = S>>(&mut self, args: I) -> &mut CommandBuilder {
        for a in args {
            self.args.push(a.into());
        }
        self
    }

    pub fn env<K: Into<String>, V: Into<String>>(
        &mut self,
        key: K,
        value: V,
    ) -> &mut CommandBuilder {
        self.envs.insert(key.into(), value.into());
        self
    }

    pub fn setsid(&mut self, enable: bool) -> &mut CommandBuilder {
        self.setsid = enable;
        self
    }

    #[cfg(unix)]
    fn handle_setsid(&self, c: &mut Command) {
        use std::os::unix::process::CommandExt;

        if self.setsid {
            unsafe {
                c.pre_exec(|| {
                    if libc::setsid() == -1 {
                        Err(std::io::Error::last_os_error())
                    } else {
                        Ok(())
                    }
                });
            }
        }
    }

    #[cfg(not(unix))]
    fn handle_setsid(&self, c: &mut Command) {
        if self.setsid {
            unsupported!()
        }
    }

    pub fn build(&self) -> Command {
        let mut c = Command::new(&self.cmd);
        for a in self.args.iter() {
            c.arg(a);
        }
        for (k, v) in self.envs.iter() {
            c.env(k, v);
        }
        self.handle_setsid(&mut c);
        c
    }
}

impl std::fmt::Display for CommandBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.cmd)?;
        for a in self.args.iter() {
            if a.contains(' ') {
                write!(f, " \"{}\"", a)?;
            } else {
                write!(f, " {}", a)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_env_single() {
        let tree = r#"{
          "env": "$.host['-hos*tname']",
          "host": {
            "-hos*tname": "localhost.localdomain"
          }
        }"#;

        let tree = NodeRef::from_json(tree).unwrap();
        let env = TaskEnv::parse(&tree.get_child_key("env").unwrap()).unwrap();
        let scope = ScopeMut::new();
        let r = resolve_env(&env, &tree, &tree, &scope).unwrap();
        assert_eq!("localhost.localdomain", r.get("HOST__HOS_TNAME").unwrap())
    }

    #[test]
    fn resolve_env_array() {
        let tree = r#"{
          "env": [
              "$.host['-hos*tname']",
              "$.prop['inn$r1']"
          ],
          "host": {
            "-hos*tname": "localhost.localdomain"
          },
          "prop": {
            "inn$r1": "inner value"
          }
        }"#;

        let tree = NodeRef::from_json(tree).unwrap();
        let env = TaskEnv::parse(&tree.get_child_key("env").unwrap()).unwrap();
        let scope = ScopeMut::new();
        let r = resolve_env(&env, &tree, &tree, &scope).unwrap();

        assert_eq!("localhost.localdomain", r.get("HOST__HOS_TNAME").unwrap());
        assert_eq!("inner value", r.get("PROP_INN_R1").unwrap());
    }

    #[test]
    fn resolve_env_map() {
        let tree = r#"{
          "env": {
              "HOST_HOSTNAME": "$.host['-hos*tname']",
              "SOME_VAR":"$.prop['inn$r']"
          },
          "host": {
            "-hos*tname": "localhost.localdomain"
          },
          "prop": {
            "inn$r": "inner value"
          }
        }"#;

        let tree = NodeRef::from_json(tree).unwrap();
        let env = TaskEnv::parse(&tree.get_child_key("env").unwrap()).unwrap();
        let scope = ScopeMut::new();
        let r = resolve_env(&env, &tree, &tree, &scope).unwrap();

        assert_eq!("localhost.localdomain", r.get("HOST_HOSTNAME").unwrap());
        assert_eq!("inner value", r.get("SOME_VAR").unwrap());
    }

    #[test]
    fn prepare_script_from_source() {
        let mut env: EnvVars = LinkedHashMap::new();
        env.insert("VAR1".into(), "var1".into());
        env.insert("USER_USERNAME".into(), "root".into());

        let mut s = Vec::new();
        prepare_script(
            SourceRef::Source("#!/usr/bin/env python\nprint 'Hello world';"),
            &[],
            Some(&env),
            None,
            &mut s,
        )
        .unwrap();
        println!("{}", unsafe { std::str::from_utf8_unchecked(&s) });
    }
}
