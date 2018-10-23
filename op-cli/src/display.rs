use kg_tree::opath::NodeSet;
use op_exec::Outcome;
use serde_json;
use serde_yaml;
use std;
use toml;

#[derive(Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq, Hash)]
pub enum DisplayFormat {
    Json,
    Yaml,
    Toml,
    Text,
    Table,
}

impl DisplayFormat {
    pub fn from(f: &str) -> DisplayFormat {
        if f.eq_ignore_ascii_case("text") || f.eq_ignore_ascii_case("txt") {
            DisplayFormat::Text
        } else if f.eq_ignore_ascii_case("tab") || f.eq_ignore_ascii_case("table") {
            DisplayFormat::Table
        } else if f.eq_ignore_ascii_case("json") {
            DisplayFormat::Json
        } else if f.eq_ignore_ascii_case("yaml") || f.eq_ignore_ascii_case("yml") {
            DisplayFormat::Yaml
        } else if f.eq_ignore_ascii_case("toml") {
            DisplayFormat::Toml
        } else {
            DisplayFormat::Text
        }
    }
}

impl std::str::FromStr for DisplayFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DisplayFormat::from(s))
    }
}

impl<'a> std::convert::From<&'a str> for DisplayFormat {
    fn from(s: &'a str) -> Self {
        DisplayFormat::from(s)
    }
}

impl std::fmt::Display for DisplayFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            DisplayFormat::Table => write!(f, "table"),
            DisplayFormat::Text => write!(f, "text"),
            DisplayFormat::Json => write!(f, "json"),
            DisplayFormat::Yaml => write!(f, "yaml"),
            DisplayFormat::Toml => write!(f, "toml"),
        }
    }
}

pub fn display_outcome(outcome: &Outcome, format: DisplayFormat) {
    match *outcome {
        Outcome::Empty => {}
        Outcome::Diff(ref diff) => {
            println!("{}", diff);
        }
        Outcome::File(ref path) => {
            println!("{:?}", path);
        }
        Outcome::Many(ref outcomes) => {
            for outcome in outcomes.iter() {
                display_outcome(outcome, format)
            }
        }
        Outcome::NodeSet(ref node_set) => {
            display_nodeset(&*node_set.lock(), format);
        }
    }
}

fn display_nodeset(ns: &NodeSet, format: DisplayFormat) {
    match format {
        DisplayFormat::Json => display_nodeset_json(ns),
        DisplayFormat::Yaml => display_nodeset_yaml(ns),
        DisplayFormat::Toml => display_nodeset_toml(ns),
        DisplayFormat::Text => display_nodeset_text(ns),
        DisplayFormat::Table => display_nodeset_table(ns),
    }
}

fn display_nodeset_json(ns: &NodeSet) {
    match *ns {
        NodeSet::Empty => {}
        NodeSet::One(ref node) => println!("{}", node.to_json_pretty()),
        NodeSet::Many(ref nodes) => println!("{}", serde_json::to_string_pretty(nodes).unwrap()),
    }
}

fn display_nodeset_yaml(ns: &NodeSet) {
    match *ns {
        NodeSet::Empty => {}
        NodeSet::One(ref node) => println!("{}", node.to_yaml()),
        NodeSet::Many(ref nodes) => println!("{}", serde_yaml::to_string(nodes).unwrap()),
    }
}

fn display_nodeset_toml(ns: &NodeSet) {
    match *ns {
        NodeSet::Empty => {}
        NodeSet::One(ref node) => println!("{}", node.to_toml()),
        NodeSet::Many(ref nodes) => println!("{}", toml::to_string(nodes).unwrap()),
    }
}

fn display_nodeset_text(ns: &NodeSet) {
    match *ns {
        NodeSet::Empty => {}
        NodeSet::One(ref node) => println!("{}", node.to_yaml()),
        NodeSet::Many(ref nodes) => println!("{}", toml::to_string(nodes).unwrap()),
    }
}

fn display_nodeset_table(ns: &NodeSet) {
    match *ns {
        NodeSet::Empty => {}
        NodeSet::One(ref node) => println!("{}", node.to_yaml()),
        NodeSet::Many(ref nodes) => println!("{}", toml::to_string(nodes).unwrap()),
    }
}
