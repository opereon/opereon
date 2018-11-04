use super::*;


#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "method")]
pub enum SshAuth {
    PublicKey {
        key_path: PathBuf,
    },
    Password {
        password: String,
    },
}

impl SshAuth {
    pub (crate) fn set_auth(&self, cmd: &mut CommandBuilder) {
        match *self {
            SshAuth::PublicKey { ref key_path } => {
                cmd.arg("-i").arg(key_path.to_str().unwrap());
            }
            SshAuth::Password { ref password } => {
                cmd.arg("-o").arg("NumberOfPasswordPrompts=1");
                cmd.env("DISPLAY", ":0");
                cmd.env("SSH_ASKPASS", "/home/outsider/workspace/opereon/target/debug/op-ask");
                cmd.env("OPEREON_PASSWD", password.as_ref());
                cmd.setsid(true);
            }
        }
    }
}


#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshDest {
    hostname: String,
    port: u16,
    username: String,
    auth: SshAuth,
}

impl SshDest {
    pub fn new<S1, S2>(hostname: S1, port: u16, username: S2, auth: SshAuth) -> SshDest
        where S1: Into<String>, S2: Into<String>
    {
        SshDest {
            hostname: hostname.into(),
            port,
            username: username.into(),
            auth,
        }
    }

    pub fn to_uri(&self) -> String {
        if self.port == 22 {
            format!("ssh://{username}@{hostname}",
                    username = self.username,
                    hostname = self.hostname)
        } else {
            format!("ssh://{username}@{hostname}:{port}",
                    username = self.username,
                    hostname = self.hostname,
                    port = self.port)
        }
    }

    pub (crate) fn to_id_string(&self) -> String {
        format!("{username}-{hostname}-{port}",
                    username = self.username,
                    hostname = self.hostname,
                    port = self.port)
    }

    pub fn hostname(&self) -> &str {
        &self.hostname
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn auth(&self) -> &SshAuth {
        &self.auth
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    mod auth {
        use super::*;

        #[test]
        fn can_serialize_public_key() {
            let a = SshAuth::PublicKey { key_path: PathBuf::from("~/.ssh/id_rsa") };
            let s = serde_json::to_string(&a).unwrap();

            assert_eq!(r#"{"method":"public-key","key_path":"~/.ssh/id_rsa"}"#, &s);
        }

        #[test]
        fn can_serialize_password() {
            let a = SshAuth::Password { password: "passw0rd".into() };
            let s = serde_json::to_string(&a).unwrap();

            assert_eq!(r#"{"method":"password","password":"passw0rd"}"#, &s);
        }
    }
}
