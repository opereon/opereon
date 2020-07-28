use crate::ops::exec::config::ExecConfig;
use kg_diag::io::fs;
use kg_diag::Severity;
use kg_diag::{BasicDiag, DiagResultExt, IntoDiagRes};
use kg_tree::diff::NodeDiffOptions;
use kg_tree::opath::{RootedResolveStrategy, TreeResolver};
use kg_tree::serial::{from_tree, to_tree};
use kg_tree::NodeRef;
use regex::{Captures, Regex};
use std::borrow::Cow;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use op_log::config::LogConfig;

pub type ConfigResult<T> = Result<T, BasicDiag>;

#[derive(Debug, Display, Detail)]
#[diag(code_offset = 1000)]
pub enum ConfigErrorDetail {
    #[display(fmt = "cannot find config in paths: {paths}")]
    NotFound { paths: String },

    #[display(fmt = "cannot parse config file '{file}'")]
    ParseFile { file: String },

    #[display(fmt = "cannot parse config")]
    ParseConf,

    #[display(fmt = "cannot resolve config interpolation")]
    InterpolationErr,

    #[display(fmt = "cannot create config")]
    DeserializationErr,
}

pub fn resolve_env_vars(input: &str) -> Cow<str> {
    use std::env;

    lazy_static! {
        static ref ENV_VAR_NAME_RE: Regex = Regex::new("\\$([A-Z_][A-Z0-9_]*)").unwrap();
    }

    ENV_VAR_NAME_RE.replace_all(input, |caps: &Captures| {
        let var_name = &caps[1];
        env::var(var_name).unwrap_or_default()
    })
}

pub fn parse_path_list(path_list: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for p in path_list.split(';') {
        let p = p.trim();
        if !p.is_empty() {
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
    diff: NodeDiffOptions,
}

impl ModelConfig {
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn cache_limit(&self) -> usize {
        self.cache_limit
    }

    pub fn diff(&self) -> &NodeDiffOptions {
        &self.diff
    }
}

impl Default for ModelConfig {
    fn default() -> Self {
        ModelConfig {
            data_dir: PathBuf::from("/var/run/opereon/data"),
            cache_limit: 10,
            diff: NodeDiffOptions::new(true, Some(5), Some(0.1)),
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
    fn read(path_list: &str) -> ConfigResult<Config> {
        let d =
            to_tree(&Config::default()).expect("Config should always be serializable to NodeRef");
        let paths = parse_path_list(path_list);
        let mut read_paths = 0;
        let mut content = String::new();
        for path in paths {
            match fs::read_to_string(&path, &mut content) {
                Ok(_) => {
                    let c: NodeRef = NodeRef::from_toml(&content).map_err_as_cause(|| {
                        ConfigErrorDetail::ParseFile {
                            file: path.to_string_lossy().to_string(),
                        }
                    })?;
                    d.extend(c, None).unwrap(); //FIXME (jc) handle errors
                    read_paths += 1;
                }
                Err(err) => {
                    //ignore non-existent paths for now
                    if err.kind() != std::io::ErrorKind::NotFound {
                        return Err(err.into());
                    }
                }
            }
        }
        if read_paths == 0 {
            return Err(ConfigErrorDetail::NotFound {
                paths: path_list.to_string(),
            }
            .into());
        }

        let mut r = TreeResolver::with_delims("${", "}");
        r.resolve_custom(RootedResolveStrategy, &d)
            .map_err_as_cause(|| ConfigErrorDetail::InterpolationErr)?;

        let conf: Self = from_tree(&d)
            .into_diag_res()
            .map_err_as_cause(|| ConfigErrorDetail::DeserializationErr)?;
        Ok(conf)
    }

    fn from_json(json: &str) -> ConfigResult<Config> {
        let d = to_tree(&Config::default()).unwrap();
        let c: NodeRef =
            NodeRef::from_json(&json).map_err_as_cause(|| ConfigErrorDetail::ParseConf)?;
        d.extend(c, None).unwrap(); //FIXME (jc) handle errors

        let mut r = TreeResolver::with_delims("${", "}");
        r.resolve_custom(RootedResolveStrategy, &d)
            .map_err_as_cause(|| ConfigErrorDetail::InterpolationErr)?;

        let conf: Self = from_tree(&d)
            .into_diag_res()
            .map_err_as_cause(|| ConfigErrorDetail::DeserializationErr)?;
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
    pub fn read(path_list: &str) -> ConfigResult<ConfigRef> {
        let config = Config::read(path_list)?;
        Ok(ConfigRef(Arc::new(config)))
    }

    pub fn from_json(json: &str) -> ConfigResult<ConfigRef> {
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
