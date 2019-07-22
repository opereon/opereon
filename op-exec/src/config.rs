use std::borrow::Cow;
use std::ops::Deref;
use std::sync::Arc;

use kg_tree::opath::{RootedResolveStrategy, TreeResolver};
use kg_tree::serial::{from_tree, to_tree};
use regex::{Captures, Regex};
use slog::Level;

use super::*;

pub fn resolve_env_vars(input: &str) -> Cow<str> {
    use std::env;

    lazy_static! {
        static ref ENV_VAR_NAME_RE: Regex = Regex::new("\\$([A-Z_][A-Z0-9_]*)").unwrap();
    }

    ENV_VAR_NAME_RE.replace_all(input, |caps: &Captures| {
        let var_name = &caps[1];
        env::var(var_name).unwrap_or(String::new())
    })
}

pub fn parse_path_list(path_list: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for p in path_list.split(';') {
        let p = p.trim();
        if p.len() > 0 {
            let s = resolve_env_vars(p);
            let path = PathBuf::from(s.as_ref());
            paths.push(path);
        }
    }
    paths
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    socket_path: PathBuf,
    pid_file_path: PathBuf,
}

impl DaemonConfig {
    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn pid_file_path(&self) -> &Path {
        &self.pid_file_path
    }
}

impl Default for DaemonConfig {
    fn default() -> Self {
        DaemonConfig {
            socket_path: PathBuf::from("/var/run/opereon/op.sock"),
            pid_file_path: PathBuf::from("/var/run/opereon/op.pid"),
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct QueueConfig {
    persist_dir: PathBuf,
}

impl QueueConfig {
    pub fn persist_dir(&self) -> &Path {
        &self.persist_dir
    }
}

impl Default for QueueConfig {
    fn default() -> Self {
        QueueConfig {
            persist_dir: PathBuf::from("/var/run/opereon/queue"),
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelConfig {
    data_dir: PathBuf,
    cache_limit: usize,
}

impl ModelConfig {
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn cache_limit(&self) -> usize {
        self.cache_limit
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        ModelConfig {
            data_dir: PathBuf::from("/var/run/opereon/data"),
            cache_limit: 10,
        }
    }
}


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Critical,
    Error,
    Warning,
    Info,
    Debug,
    Trace,
}
impl Into<Level> for LogLevel {
    fn into(self) -> Level {
        match self {
            LogLevel::Trace => Level::Trace,
            LogLevel::Debug => Level::Debug,
            LogLevel::Info => Level::Info,
            LogLevel::Warning => Level::Warning,
            LogLevel::Error => Level::Error,
            LogLevel::Critical => Level::Critical,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LogConfig {
    level: LogLevel,
    log_path: PathBuf,
}

impl LogConfig {
    pub fn level(&self) -> &LogLevel {
        &self.level
    }

    pub fn log_path(&self) -> &Path {
        &self.log_path
    }
}

impl Default for LogConfig {
    fn default() -> Self {
        LogConfig {
            level: LogLevel::Info,
            log_path: PathBuf::from("/var/log/opereon/opereon.log"),
        }
    }
}


#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    run_dir: PathBuf,
    data_dir: PathBuf,
    daemon: DaemonConfig,
    log: LogConfig,
    queue: QueueConfig,
    model: ModelConfig,
    exec: ExecConfig,
}

impl Config {
    //FIXME (jc) add proper error type
    fn read(path_list: &str) -> IoResult<Config> {
        let d = to_tree(&Config::default()).unwrap();
        let paths = parse_path_list(path_list);
        let mut read_paths = 0;
        let mut content = String::new();
        for path in paths {
            match fs::read_to_string(path, &mut content) {
                Ok(_) => {
                    let c: NodeRef = NodeRef::from_toml(&content).unwrap(); //FIXME (jc) handle parse errors
                    d.extend(c, None).unwrap(); //FIXME (jc) handle errors
                    read_paths += 1;
                }
                Err(err) => {
                    //ignore non-existent paths for now
                    if err.kind() != std::io::ErrorKind::NotFound {
                        return Err(err);
                    }
                }
            }
        }
        if read_paths == 0 {
            return Err(std::io::ErrorKind::NotFound.into());
        }

        let mut r = TreeResolver::with_delims("${", "}");
        r.resolve_custom(RootedResolveStrategy, &d);

        let conf: Self = from_tree(&d).unwrap(); //FIXME (jc) handle errors
        Ok(conf)
    }

    //FIXME (jc) add proper error type
    fn from_json(json: &str) -> Result<Config, std::io::Error> {
        let d = to_tree(&Config::default()).unwrap();
        let c: NodeRef = NodeRef::from_json(&json).unwrap(); //FIXME (jc) handle parse errors
        d.extend(c, None).unwrap(); //FIXME (jc) handle errors

        let mut r = TreeResolver::with_delims("${", "}");
        r.resolve_custom(RootedResolveStrategy, &d);

        let conf: Self = from_tree(&d).unwrap(); //FIXME (jc) handle errors
        Ok(conf)
    }

    pub fn run_dir(&self) -> &Path {
        &self.run_dir
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn daemon(&self) -> &DaemonConfig {
        &self.daemon
    }

    pub fn queue(&self) -> &QueueConfig {
        &self.queue
    }

    pub fn model(&self) -> &ModelConfig {
        &self.model
    }

    pub fn exec(&self) -> &ExecConfig {
        &self.exec
    }

    pub fn log(&self) -> &LogConfig {
        &self.log
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            run_dir: PathBuf::from("/var/run/opereon"),
            data_dir: PathBuf::from("/var/lib/opereon"),
            daemon: DaemonConfig::default(),
            log: LogConfig::default(),
            queue: QueueConfig::default(),
            model: ModelConfig::default(),
            exec: ExecConfig::default(),
        }
    }
}

impl std::fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", toml::to_string_pretty(self).unwrap())
    }
}

#[derive(Debug, Clone)]
pub struct ConfigRef(Arc<Config>);

impl ConfigRef {
    pub fn read(path_list: &str) -> IoResult<ConfigRef> {
        let config = Config::read(path_list)?;
        Ok(ConfigRef(Arc::new(config)))
    }

    pub fn from_json(json: &str) -> Result<ConfigRef, std::io::Error> {
        let config = Config::from_json(json)?;
        Ok(ConfigRef(Arc::new(config)))
    }
}

impl Default for ConfigRef {
    fn default() -> Self {
        ConfigRef(Arc::new(Config::default()))
    }
}

impl Deref for ConfigRef {
    type Target = Config;

    fn deref(&self) -> &<Self as Deref>::Target {
        &*self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_path_list_should_resolve_env_vars() {
        std::env::set_var("VAR1_", "var1_value");
        let paths = parse_path_list("$VAR1_/.opereon/config.toml");
        std::env::remove_var("VAR1_");

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], Path::new("var1_value/.opereon/config.toml"));
    }
}
