use super::*;

use self::display::DisplayFormat;
use std::path::PathBuf;
use structopt::clap::AppSettings;

fn parse_key_value(s: &str) -> Result<(String, String), String> {
    match s.find('=') {
        Some(pos) => Ok((s[..pos].into(), s[pos + 1..].into())),
        None => Err("argument must be in form -Akey=value".into()),
    }
}

fn parse_ssh_url(s: &str) -> Result<Url, String> {
    if s.starts_with("ssh://") {
        Url::parse(s).map_err(|e| e.to_string())
    } else {
        Url::parse(&format!("ssh://{}", s)).map_err(|e| e.to_string())
    }
}

#[derive(Debug, StructOpt)]
#[structopt(
    name = "op",
    about = "OPEREON - System configuration automation.\nCopyright (c) Kodegenix Sp z o.o. (http://www.kodegenix.pl).",
    settings = &[AppSettings::InferSubcommands, AppSettings::ColoredHelp],
)]
pub struct Opts {
    /// Path(s) to the config file
    #[structopt(
        short = "c",
        long = "config",
        name = "PATH",
        default_value = "/etc/opereon/config.toml; $HOME/.opereon/config/config.toml"
    )]
    pub config_file_path: String,

    /// Path to model directory
    #[structopt(
        short = "m",
        long = "model-dir",
        name = "MODEL_DIR",
        default_value = "."
    )]
    pub model_dir_path: String,

    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: u8,

    #[structopt(subcommand)]
    pub command: Command,
}

#[derive(Debug, StructOpt)]
pub enum Command {
    /// Prints config to the output
    #[structopt(
        name = "config",
        setting = AppSettings::ColoredHelp
    )]
    Config {
        /// Output format
        #[structopt(
            short = "f",
            long = "format",
            possible_values = &["json","yaml","toml"],
            case_insensitive = true,
            default_value = "toml"
        )]
        format: DisplayFormat,
    },
    /// Commit current model
    #[structopt(
        name = "commit",
        setting = AppSettings::ColoredHelp,
    )]
    Commit {
        /// Optional path to read model from. By default current directory model is used.
        #[structopt(name = "MESSAGE", default_value = "Model update")]
        message: String,
    },
    /// Query model
    #[structopt(
        name = "query",
        setting = AppSettings::ColoredHelp,
    )]
    Query {
        /// Output format
        #[structopt(
            short = "f",
            long = "format",
            possible_values = &["json","yaml","toml","text","table"],
            case_insensitive = true,
            default_value = "yaml"
        )]
        format: DisplayFormat,
        /// Model path, defaults to current working directory
        #[structopt(short = "m", long = "model", default_value = "@")]
        model: RevPath,
        /// Query expression
        #[structopt(name = "OPATH")]
        expr: String,
    },
    /// Test model
    #[structopt(
        name = "test",
        setting = AppSettings::ColoredHelp,
    )]
    Test {
        /// Output format
        #[structopt(
            short = "f",
            long = "format",
            possible_values = &["json","yaml","toml","text","table"],
            case_insensitive = true,
            default_value = "yaml"
        )]
        format: DisplayFormat,
        /// Model path, defaults to current working directory
        #[structopt(name = "MODEL", default_value = "@")]
        model: RevPath,
    },
    /// Compare two model versions
    #[structopt(
        name = "diff",
        setting = AppSettings::ColoredHelp,
    )]
    Diff {
        /// Output format
        #[structopt(
            short = "f",
            long = "format",
            possible_values = &["json","yaml","toml","text","table"],
            case_insensitive = true,
            default_value = "yaml"
        )]
        format: DisplayFormat,
        /// Target model path, defaults to current working directory
        #[structopt(name = "TARGET", default_value = "@")]
        target: RevPath,
        /// Source model path, defaults to current model
        #[structopt(name = "SOURCE", default_value = "HEAD")]
        source: RevPath,
    },
    /// Update model to a new version
    #[structopt(
        name = "update",
        setting = AppSettings::ColoredHelp,
    )]
    Update {
        /// Output format
        #[structopt(
            short = "f",
            long = "format",
            possible_values = &["json","yaml","toml","text","table"],
            case_insensitive = true,
            default_value = "yaml"
        )]
        format: DisplayFormat,
        /// When set this flags prevents from actually executing any actions in hosts
        #[structopt(short = "d", long = "dry-run")]
        dry_run: bool,
        /// Target model path, defaults to current working directory
        #[structopt(name = "TARGET", default_value = "@")]
        target: RevPath,
        /// Source model path, defaults to current model(HEAD)
        #[structopt(name = "SOURCE", default_value = "HEAD")]
        source: RevPath,
    },
    /// Run checks from a model
    #[structopt(
        name = "check",
        setting = AppSettings::ColoredHelp,
    )]
    Check {
        /// Model path, defaults to current model
        #[structopt(name = "MODEL", default_value = "@")]
        model: RevPath,
        /// Check name filter expression
        #[structopt(short = "n", long = "name")]
        filter: Option<String>,
        /// When set this flags prevents from actually executing any actions in hosts
        #[structopt(short = "d", long = "dry-run")]
        dry_run: bool,
    },
    /// Run probe from a model
    #[structopt(
        name = "probe",
        setting = AppSettings::ColoredHelp,
    )]
    Probe {
        /// SSH connection url to remote host being probed, for example ssh://root@example.org:22
        #[structopt(name = "URL", parse(try_from_str = parse_ssh_url))]
        url: Url,
        /// Password for SSH authentication
        #[structopt(short = "P", long = "password", group = "ssh_auth")]
        password: Option<String>,
        /// Path to an identity file for SSH authentication
        #[structopt(short = "i", group = "ssh_auth")]
        identity_file: Option<PathBuf>,
        /// Probe name filter expression
        #[structopt(short = "n", long = "name")]
        filter: Option<String>,
        /// Arguments for the probe
        #[structopt(short = "A", parse(try_from_str = parse_key_value))]
        args: Vec<(String, String)>,
        /// Model path, defaults to current model
        #[structopt(name = "MODEL", default_value = "@")]
        model: RevPath,
    },
    /// Execute shell command on remote host(s)
    #[structopt(
        name = "remote",
        setting = AppSettings::ColoredHelp,
    )]
    Remote {
        /// Query expression. Determines target hosts. Defaults to all hosts from current model
        #[structopt(name = "OPATH", short = "h", long = "hosts", default_value = "$$hosts")]
        expr: String,
        /// Command to execute on remote hosts
        #[structopt(name = "COMMAND", raw(true))]
        command: Vec<String>,
        /// Model path, defaults to current working directory
        #[structopt(short = "m", long = "model", default_value = "@")]
        model: RevPath,
    },
    /// Execute prepared work package
    #[structopt(
        name = "exec",
        setting = AppSettings::ColoredHelp,
    )]
    Exec {
        /// Work path, defaults to current working directory
        #[structopt(name = "PATH", default_value = ".", parse(from_os_str))]
        path: PathBuf,
    },
    /// Initialize empty opereon model
    #[structopt(
        name = "init",
        setting = AppSettings::ColoredHelp,
    )]
    Init {
        /// Path to new model, defaults to current working directory
        #[structopt(name = "PATH", default_value = ".", parse(from_os_str))]
        path: PathBuf,
    },
}
