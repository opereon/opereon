use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SshConfig {
    socket_dir: PathBuf,
    ssh_cmd: String,
    runas_cmd: String,
    shell_cmd: String,
    cache_limit: usize,
}

impl SshConfig {
    pub fn socket_dir(&self) -> &Path {
        &self.socket_dir
    }

    pub fn ssh_cmd(&self) -> &str {
        &self.ssh_cmd
    }

    pub fn runas_cmd(&self) -> &str {
        &self.runas_cmd
    }

    pub fn shell_cmd(&self) -> &str {
        &self.shell_cmd
    }

    pub fn cache_limit(&self) -> usize {
        self.cache_limit
    }

    pub fn set_socket_dir(&mut self, socket_dir: &Path) {
        self.socket_dir = socket_dir.to_path_buf();
    }
}

impl Default for SshConfig {
    fn default() -> Self {
        SshConfig {
            socket_dir: PathBuf::from("/var/run/opereon/ssh"),
            ssh_cmd: "/bin/ssh".into(),
            runas_cmd: "/bin/sudo".into(),
            shell_cmd: "/bin/bash".into(),
            cache_limit: 10,
        }
    }
}
