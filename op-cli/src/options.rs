use cli_client::display::DisplayFormat;
use std::path::PathBuf;
use structopt::clap::AppSettings;

use op_exec::{DiffMethod, ModelPath};

#[derive(Debug, StructOpt)]
#[structopt(
    name = "op",
    author = "",
    about = "OPEREON - System configuration automation.\nCopyright (c) Kodegenix Sp z o.o. (http://www.kodegenix.pl).",
    raw(setting = "AppSettings::InferSubcommands"),
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

    /// Verbose mode (-v, -vv, -vvv, etc.)
    #[structopt(short = "v", long = "verbose", parse(from_occurrences))]
    pub verbose: u8,

    /// Execute command locally, without connecting to Operon daemon.
    #[structopt(short = "L", long = "local")]
    pub local: bool,

    /// Execute command on running Operon daemon.
    #[structopt(short = "R", long = "remote")]
    pub remote: bool,

    #[structopt(subcommand)]
    pub command: Command,
}

#[derive(Debug, StructOpt)]
pub enum Command {
    //////////////////////////////// Daemon options ////////////////////////////////
    /// Start Opereon service
    #[structopt(name = "start", author = "")]
    Start {
        /// Run opereon service in foreground instead of as a daemon.
        /// This is useful for running operon in docker container.
        #[structopt(short = "f", long = "foreground")]
        foreground: bool,
    },

    /// Stop Opereon service running
    #[structopt(name = "stop", author = "")]
    Stop,

    /// List reachable Opereon instances.
    #[structopt(name = "nodes", author = "")]
    Nodes,

    /// Prints config to the output
    #[structopt(name = "config", author = "")]
    Config {
        /// Output format
        #[structopt(
            short = "f",
            long = "format",
            raw(
                possible_values = r#"&["json","yaml","toml"]"#,
                case_insensitive = "true"
            ),
            default_value = "toml"
        )]
        format: DisplayFormat,
    },
    /// Output list of known model versions
    #[structopt(name = "list", author = "")]
    List {
        /// Output format
        #[structopt(
            short = "f",
            long = "format",
            raw(
                possible_values = r#"&["json","yaml","toml","text","table"]"#,
                case_insensitive = "true"
            ),
            default_value = "table"
        )]
        format: DisplayFormat,
    },
    /// Store model from given path as current version
    #[structopt(name = "store", author = "")]
    Store {
        /// Optional path to read model from. By default current directory model is used.
        #[structopt(name = "PATH", parse(from_os_str))]
        path: Option<PathBuf>,
    },
    /// Query model
    #[structopt(name = "query", author = "")]
    Query {
        /// Output format
        #[structopt(
            short = "f",
            long = "format",
            raw(
                possible_values = r#"&["json","yaml","toml","text","table"]"#,
                case_insensitive = "true"
            ),
            default_value = "yaml"
        )]
        format: DisplayFormat,
        /// Model path, defaults to current working directory
        #[structopt(short = "m", long = "model", default_value = ".")]
        model: ModelPath,
        /// Query expression
        #[structopt(name = "OPATH")]
        expr: String,
    },
    /// Test model
    #[structopt(name = "test", author = "")]
    Test {
        /// Output format
        #[structopt(
            short = "f",
            long = "format",
            raw(
                possible_values = r#"&["json","yaml","toml","text","table"]"#,
                case_insensitive = "true"
            ),
            default_value = "yaml"
        )]
        format: DisplayFormat,
        /// Model path, defaults to current working directory
        #[structopt(name = "MODEL", default_value = ".")]
        model: ModelPath,
    },
    /// Compare two model versions
    #[structopt(name = "diff", author = "")]
    Diff {
        /// Output format
        #[structopt(
            short = "f",
            long = "format",
            raw(
                possible_values = r#"&["json","yaml","toml","text","table"]"#,
                case_insensitive = "true"
            ),
            default_value = "yaml"
        )]
        format: DisplayFormat,
        /// Diff method
        #[structopt(
            short = "m",
            long = "method",
            raw(
                possible_values = r#"&["minimal","full"]"#,
                case_insensitive = "true"
            ),
            default_value = "minimal"
        )]
        method: DiffMethod,
        /// Target model path, defaults to current working directory
        #[structopt(name = "TARGET", default_value = ".")]
        target: ModelPath,
        /// Source model path, defaults to current model
        #[structopt(name = "SOURCE", default_value = "@")]
        source: ModelPath,
    },
    /// Update model to a new version
    #[structopt(name = "update", author = "")]
    Update {
        /// Output format
        #[structopt(
            short = "f",
            long = "format",
            raw(
                possible_values = r#"&["json","yaml","toml","text","table"]"#,
                case_insensitive = "true"
            ),
            default_value = "yaml"
        )]
        format: DisplayFormat,
        /// When set this flags prevents from actually executing any actions in hosts
        #[structopt(short = "d", long = "dry-run")]
        dry_run: bool,
        /// Target model path, defaults to current working directory
        #[structopt(name = "TARGET", default_value = ".")]
        target: ModelPath,
        /// Source model path, defaults to current model
        #[structopt(name = "SOURCE", default_value = "@")]
        source: ModelPath,
    },
    /// Run checks from a model
    #[structopt(name = "check", author = "")]
    Check {
        /// Model path, defaults to current model
        #[structopt(name = "MODEL", default_value = "@")]
        model: ModelPath,
        /// Name filter expression
        #[structopt(short = "n", long = "name")]
        filter: Option<String>,
        /// When set this flags prevents from actually executing any actions in hosts
        #[structopt(short = "d", long = "dry-run")]
        dry_run: bool,
    },
    /// Execute prepared work package
    #[structopt(name = "exec", author = "")]
    Exec {
        /// Work path, defaults to current working directory
        #[structopt(name = "PATH", default_value = ".", parse(from_os_str))]
        path: PathBuf,
    },
}
