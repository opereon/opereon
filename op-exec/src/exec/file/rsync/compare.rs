use std::process::{Output, Stdio};

use regex::Regex;

use super::*;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModFlags {
    checksum: Option<bool>,
    size: Option<bool>,
    mod_time: Option<bool>,
    perms: Option<bool>,
    owner: Option<bool>,
    group: Option<bool>,
    update_time: Option<bool>,
    acl: Option<bool>,
    ext_attr: Option<bool>,
}

impl ModFlags {
    pub fn parse(s: &[u8]) -> RsyncParseResult<ModFlags> {
        fn parse_flag(b: u8, on: u8) -> RsyncParseResult<Option<bool>> {
            if b == on {
                Ok(Some(true))
            } else if b == b' ' || b == b'.' || b == b'+' {
                Ok(Some(false))
            } else if b == b'?' {
                Ok(None)
            } else {
                RsyncParseErrorDetail::custom_line(line!())
            }
        }

        if s.len() != 9 {
            RsyncParseErrorDetail::custom_output(line!(), String::from_utf8_lossy(s).to_string())
        } else {
            Ok(ModFlags {
                checksum: parse_flag(s[0], b'c')?,
                size: parse_flag(s[1], b's')?,
                mod_time: parse_flag(s[2], b't')?,
                perms: parse_flag(s[3], b'p')?,
                owner: parse_flag(s[4], b'o')?,
                group: parse_flag(s[5], b'g')?,
                update_time: parse_flag(s[6], b'u')?,
                acl: parse_flag(s[7], b'a')?,
                ext_attr: parse_flag(s[8], b'x')?,
            })
        }
    }

    pub fn is_modified_content(&self) -> bool {
        self.checksum == Some(true) || self.size == Some(true)
    }

    pub fn is_modified_chmod(&self) -> bool {
        self.perms == Some(true)
    }

    pub fn is_modified_chown(&self) -> bool {
        self.owner == Some(true) || self.group == Some(true)
    }

    pub fn checksum(&self) -> Option<bool> {
        self.checksum
    }

    pub fn size(&self) -> Option<bool> {
        self.size
    }

    pub fn mod_time(&self) -> Option<bool> {
        self.mod_time
    }

    pub fn perms(&self) -> Option<bool> {
        self.perms
    }

    pub fn owner(&self) -> Option<bool> {
        self.owner
    }

    pub fn group(&self) -> Option<bool> {
        self.group
    }

    pub fn update_time(&self) -> Option<bool> {
        self.update_time
    }

    pub fn acl(&self) -> Option<bool> {
        self.acl
    }

    pub fn ext_attr(&self) -> Option<bool> {
        self.ext_attr
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum State {
    /// Item is identical in both source and destination locations
    Identical,
    /// Item is modified between source and destination locations
    Modified(ModFlags),
    /// Item exists in source location and it is missing from destination location
    Missing,
    /// Item exists in destination location and it is missing from source location
    Extraneous,
}

impl State {
    pub fn is_modified_content(&self) -> bool {
        match *self {
            State::Identical => false,
            State::Modified(flags) => flags.is_modified_content(),
            State::Missing => true,
            State::Extraneous => true,
        }
    }

    pub fn is_modified_chmod(&self) -> bool {
        match *self {
            State::Identical => false,
            State::Modified(flags) => flags.is_modified_chmod(),
            State::Missing => true,
            State::Extraneous => true,
        }
    }

    pub fn is_modified_chown(&self) -> bool {
        match *self {
            State::Identical => false,
            State::Modified(flags) => flags.is_modified_chown(),
            State::Missing => true,
            State::Extraneous => true,
        }
    }

    pub fn mod_flags(&self) -> Option<ModFlags> {
        match *self {
            State::Modified(flags) => Some(flags),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum FileType {
    File,
    Dir,
    Symlink,
    Device,
    Special,
}

impl FileType {
    pub fn parse(t: u8) -> RsyncParseResult<FileType> {
        match t {
            b'f' => Ok(FileType::File),
            b'd' => Ok(FileType::Dir),
            b'L' => Ok(FileType::Symlink),
            b'D' => Ok(FileType::Device),
            b'S' => Ok(FileType::Special),
            _ => RsyncParseErrorDetail::custom_line(line!()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DiffInfo {
    state: State,
    file_type: Option<FileType>,
    file_path: PathBuf,
    /// Size of file in destination location
    file_size: FileSize,
}

impl DiffInfo {
    pub fn parse(
        details: &[u8],
        file_path: &str,
        file_size: FileSize,
    ) -> RsyncParseResult<DiffInfo> {
        let (file_type, state) = {
            if details == b"*deleting  " {
                (None, State::Extraneous)
            } else {
                let file_type = FileType::parse(details[1])?;
                let mod_flags = &details[2..];

                let state = match mod_flags {
                    b"+++++++++" => State::Missing,
                    b"         " => State::Identical,
                    _ => State::Modified(ModFlags::parse(mod_flags)?),
                };
                (Some(file_type), state)
            }
        };

        Ok(DiffInfo {
            file_path: file_path.into(),
            file_type,
            state,
            file_size,
        })
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    pub fn file_type(&self) -> Option<FileType> {
        self.file_type
    }

    pub fn file_size(&self) -> FileSize {
        self.file_size
    }

    pub fn state(&self) -> &State {
        &self.state
    }
}

fn build_compare_cmd(
    config: &RsyncConfig,
    params: &RsyncParams,
    checksum: bool,
) -> RsyncResult<Command> {
    let mut rsync_cmd = params.to_cmd(config);

    rsync_cmd
        .arg("--verbose")
        .arg("--recursive")
        .arg("--dry-run") // perform a trial run with no changes made
        .arg("--super") // assume super-user rights. Necessary for owner checking
        .arg("--archive") // equals -rlptgoD (no -H,-A,-X)
        .arg("--delete") // delete extraneous files from dest dirs
        .arg("-ii") // output unchanged files
        .arg("--out-format=###%i [%f][%l]") // log format described in https://download.samba.org/pub/rsync/rsyncd.conf.html
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if checksum {
        rsync_cmd.arg("--checksum"); // skip based on checksum, not mod-time & size.
    }
    Ok(rsync_cmd)
}

pub fn rsync_compare(
    config: &RsyncConfig,
    params: &RsyncParams,
    checksum: bool,
) -> RsyncResult<Vec<DiffInfo>> {
    let mut rsync_cmd = build_compare_cmd(config, params, checksum)?;
    eprintln!("rsync_cmd = {:?}", rsync_cmd);
    let output = rsync_cmd.output().map_err(RsyncErrorDetail::spawn_err)?;

    let Output {
        status,
        stdout,
        stderr,
    } = output;

    match status.code() {
        None => Err(RsyncErrorDetail::RsyncTerminated.into()),
        Some(0) => {
            let output = String::from_utf8_lossy(&stdout);
            parse_output(&output)
        }
        Some(_c) => {
            let output = String::from_utf8_lossy(&stderr);
            RsyncErrorDetail::process_exit(output.to_string())
        }
    }
}

fn parse_output(output: &str) -> RsyncParseResult<Vec<DiffInfo>> {
    let mut diffs = Vec::new();

    let items = output.lines().filter_map(|line| {
        if line.starts_with("###") && line.len() > 15 {
            Some((line[3..14].as_bytes(), &line[15..]))
        } else {
            None
        }
    });

    let file_reg = Regex::new(r"[\[\]]").unwrap();

    for (details, rest) in items {
        let file_info = file_reg
            .split(rest)
            .filter(|s| !s.is_empty())
            .collect::<Vec<&str>>();

        if file_info.len() != 2 {
            return RsyncParseErrorDetail::custom_output(line!(), output.to_string());
        }

        let file_path = file_info[0];
        let file_size =
            file_info[1]
                .parse::<FileSize>()
                .map_err(|_e| RsyncParseErrorDetail::Custom {
                    line: line!(),
                    output: output.to_string(),
                })?;

        let diff = DiffInfo::parse(details, file_path, file_size)?;
        diffs.push(diff);
    }

    Ok(diffs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use op_test_helpers::UnwrapDisplay;

    #[test]
    fn compare_cmd() {
        let expected = r#""/bin/rsync" "/home/wiktor/Desktop/opereon/resources/model/proc/hosts_file/etc/hosts" "127.0.0.1:/etc/hosts" "--chmod" "u+rw,g+r,o+r" "--group" "--owner" "--chown" "root:root" "--verbose" "--recursive" "--dry-run" "--super" "--archive" "--delete" "-ii" "--out-format=###%i [%f][%l]""#;
        let cfg = RsyncConfig::default();
        let mut params = RsyncParams::new(
            "/home/wiktor/Desktop/opereon/resources/model/",
            "/home/wiktor/Desktop/opereon/resources/model/proc/hosts_file/etc/hosts",
            "127.0.0.1:/etc/hosts",
        );
        params.chmod("u+rw,g+r,o+r").chown("root:root");

        let cmd = build_compare_cmd(&cfg, &params, false).unwrap_disp();

        assert_eq!(expected, format!("{:?}", cmd));
    }
}
