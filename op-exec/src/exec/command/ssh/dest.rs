use url::Url;

use super::*;

#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "method")]
pub enum SshAuth {
    Default,
    PublicKey { identity_file: PathBuf },
    Password { password: String },
}

impl SshAuth {
    pub(crate) fn set_auth(&self, cmd: &mut CommandBuilder) {
        lazy_static! {
            static ref OP_ASK_PATH: PathBuf = {
                let mut path = std::env::current_exe().unwrap();
                path.set_file_name("op-ask");
                path
            };
        }

        match *self {
            SshAuth::Default => {}
            SshAuth::PublicKey { ref identity_file } => {
                cmd.arg("-i").arg(identity_file.to_str().unwrap());
            }
            SshAuth::Password { ref password } => {
                cmd.arg("-o").arg("NumberOfPasswordPrompts=1");
                cmd.env("DISPLAY", ":0");
                cmd.env("SSH_ASKPASS", OP_ASK_PATH.display().to_string());
                cmd.env("OPEREON_PASSWD", password.to_owned());
                cmd.setsid(true);
            }
        }
    }
}

impl Default for SshAuth {
    fn default() -> Self {
        Self::Default
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SshDest {
    hostname: String,
    port: u16,
    username: String,
    auth: SshAuth,
}

impl SshDest {
    pub fn new<S1, S2>(hostname: S1, port: u16, username: S2, auth: SshAuth) -> SshDest
    where
        S1: Into<String>,
        S2: Into<String>,
    {
        SshDest {
            hostname: hostname.into(),
            port,
            username: username.into(),
            auth,
        }
    }

    pub fn from_url(url: &Url, auth: SshAuth) -> SshDest {
        let hostname = url.host().unwrap().to_string();
        let username = match url.username() {
            "" => users::get_current_username()
                .unwrap()
                .to_str()
                .unwrap()
                .to_string(),
            u @ _ => u.to_string(),
        };
        let port = url.port().unwrap_or(22);

        SshDest {
            hostname,
            port,
            username,
            auth,
        }
    }

    pub fn to_url(&self) -> String {
        if self.port == 22 {
            format!(
                "ssh://{username}@{hostname}",
                username = self.username,
                hostname = self.hostname
            )
        } else {
            format!(
                "ssh://{username}@{hostname}:{port}",
                username = self.username,
                hostname = self.hostname,
                port = self.port
            )
        }
    }

    pub fn set_dest(&self, target: bool, cmd: &mut CommandBuilder) {
        if target {
            cmd.arg(format!(
                "{username}@{hostname}",
                username = self.username,
                hostname = self.hostname
            ));
        }

        if self.port != 22 {
            cmd.arg("-p").arg(self.port.to_string());
        }
        self.auth.set_auth(cmd);
    }

    pub(crate) fn to_id_string(&self) -> String {
        format!(
            "{username}-{hostname}-{port}",
            username = self.username,
            hostname = self.hostname,
            port = self.port
        )
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn set_hostname<S: Into<String>>(&mut self, hostname: S) {
        self.hostname = hostname.into();
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn set_port(&mut self, port: u16) {
        self.port = port;
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn set_username<S: Into<String>>(&mut self, username: S) {
        self.username = username.into();
    }

    pub fn set_username_current(&mut self) {
        self.username = users::get_current_username()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
    }

    pub fn auth(&self) -> &SshAuth {
        &self.auth
    }

    pub fn auth_mut(&mut self) -> &mut SshAuth {
        &mut self.auth
    }

    pub fn set_auth(&mut self, auth: SshAuth) {
        self.auth = auth;
    }
}

impl Default for SshDest {
    fn default() -> Self {
        SshDest {
            hostname: String::new(),
            port: 22,
            username: String::new(),
            auth: SshAuth::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod auth {
        use super::*;

        #[test]
        fn can_serialize_public_key() {
            let a = SshAuth::PublicKey {
                identity_file: PathBuf::from("~/.ssh/id_rsa"),
            };
            let s = serde_json::to_string(&a).unwrap();

            assert_eq!(
                r#"{"method":"public-key","identity_file":"~/.ssh/id_rsa"}"#,
                &s
            );
        }

        #[test]
        fn can_serialize_password() {
            let a = SshAuth::Password {
                password: "passw0rd".into(),
            };
            let s = serde_json::to_string(&a).unwrap();

            assert_eq!(r#"{"method":"password","password":"passw0rd"}"#, &s);
        }
    }
}
