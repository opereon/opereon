use std::cell::{Ref, RefCell};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use git2::{ObjectType, Repository, TreeWalkMode, TreeWalkResult};
use globset::{Candidate, Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use toml;

use super::*;

pub static DEFAULT_CONFIG_FILENAME: &'static str = ".operc";

#[derive(Debug, PartialEq, Eq, Clone, Deserialize, Serialize)]
pub struct Include {
    path: PathBuf,
    file_type: Option<FileType>,
    item: Opath,
    mapping: Opath,
}

impl Include {
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[allow(dead_code)]
    pub fn file_type(&self) -> Option<FileType> {
        self.file_type
    }

    pub fn item(&self) -> &Opath {
        &self.item
    }

    pub fn mapping(&self) -> &Opath {
        &self.mapping
    }

    pub fn matches_file_type(&self, file_type: FileType) -> bool {
        self.file_type.is_none() || self.file_type.unwrap() == file_type
    }

    fn with_base_path(mut self, base: &Path) -> Include {
        self.path = base.join(&self.path);
        self
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Deserialize, Serialize)]
pub struct Exclude {
    path: PathBuf,
    file_type: Option<FileType>,
}

impl Exclude {
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[allow(dead_code)]
    pub fn file_type(&self) -> Option<FileType> {
        self.file_type
    }

    pub fn matches_file_type(&self, file_type: FileType) -> bool {
        self.file_type.is_none() || self.file_type.unwrap() == file_type
    }

    fn with_base_path(mut self, base: &Path) -> Exclude {
        self.path = base.join(&self.path);
        self
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    inherit_excludes: Option<bool>,
    inherit_includes: Option<bool>,
    inherit_overrides: Option<bool>,
    #[serde(rename = "exclude")]
    excludes: Vec<Exclude>,
    #[serde(rename = "include")]
    includes: Vec<Include>,
    overrides: LinkedHashMap<Opath, Opath>,

    #[serde(skip)]
    include_globset: RefCell<Option<GlobSet>>,
    #[serde(skip)]
    exclude_globset: RefCell<Option<GlobSet>>,
}

#[inline]
fn build_glob(path: &Path) -> Glob {
    let path = path.to_str().unwrap(); //FIXME (jc) handle utf8 errors
    GlobBuilder::new(path)
        .case_insensitive(false)
        .literal_separator(true)
        .build()
        .unwrap()
}

//FIXME (jc) add Include::check(), Exclude::check() and Config::check() for glob errors, add checking and reporting glob syntax errors to the user somehow
impl Config {
    fn empty() -> Self {
        Config {
            inherit_excludes: None,
            inherit_includes: None,
            inherit_overrides: None,
            excludes: Vec::new(),
            includes: Vec::new(),
            overrides: LinkedHashMap::new(),

            include_globset: RefCell::new(None),
            exclude_globset: RefCell::new(None),
        }
    }

    fn standard() -> Self {
        Config {
            inherit_excludes: Some(true),
            inherit_includes: Some(true),
            inherit_overrides: Some(true),
            excludes: vec![
                Exclude {
                    path: "**/.*/**".into(),
                    file_type: None,
                },
            ],
            includes: vec![
                Include {
                    path: "**/*".into(),
                    file_type: Some(FileType::Dir),
                    item: Opath::parse("map()").unwrap(),
                    mapping: Opath::parse("$.find(array($item.@file_path_components[:-2]).join('.', '\"')).set($item.@file_name, $item)").unwrap(),
                },
                Include {
                    path: "**/_.{yaml,yml,toml,json}".into(),
                    file_type: Some(FileType::File),
                    item: Opath::parse("loadFile(@file_path, @file_ext)").unwrap(),
                    mapping: Opath::parse("$.find(array($item.@file_path_components[:-2]).join('.', '\"')).extend($item)").unwrap(),
                },
                Include {
                    path: "**/*.{yaml,yml,toml,json}".into(),
                    file_type: Some(FileType::File),
                    item: Opath::parse("loadFile(@file_path, @file_ext)").unwrap(),
                    mapping: Opath::parse("$.find(array($item.@file_path_components[:-2]).join('.', '\"')).set($item.@file_stem, $item)").unwrap(),
                },
                Include {
                    path: "**/*".into(),
                    file_type: Some(FileType::File),
                    item: Opath::parse("loadFile(@file_path, 'text')").unwrap(),
                    mapping: Opath::parse("$.find(array($item.@file_path_components[:-2]).join('.', '\"')).set($item.@file_stem, $item)").unwrap(),
                },
            ],
            overrides: LinkedHashMap::new(),
            include_globset: RefCell::new(None),
            exclude_globset: RefCell::new(None),
        }
    }

    #[allow(dead_code)]
    pub fn excludes(&self) -> &Vec<Exclude> {
        &self.excludes
    }

    #[allow(dead_code)]
    pub fn includes(&self) -> &Vec<Include> {
        &self.includes
    }

    pub fn overrides(&self) -> &LinkedHashMap<Opath, Opath> {
        &self.overrides
    }

    fn exclude_globset(&self) -> Ref<GlobSet> {
        if self.exclude_globset.borrow().is_none() {
            let mut b = GlobSetBuilder::new();
            for exclude in self.excludes.iter() {
                let g = build_glob(exclude.path());
                b.add(g);
            }
            *self.exclude_globset.borrow_mut() = Some(b.build().unwrap())
        }
        Ref::map(self.exclude_globset.borrow(), |g| g.as_ref().unwrap())
    }

    fn include_globset(&self) -> Ref<GlobSet> {
        if self.include_globset.borrow().is_none() {
            let mut b = GlobSetBuilder::new();
            for include in self.includes.iter() {
                let g = build_glob(include.path());
                b.add(g);
            }
            *self.include_globset.borrow_mut() = Some(b.build().unwrap())
        }
        Ref::map(self.include_globset.borrow(), |g| g.as_ref().unwrap())
    }

    pub fn find_include(&self, path_rel: &Path, file_type: FileType) -> Option<&Include> {
        debug_assert!(path_rel.is_relative());

        let cpath = Candidate::new(path_rel);
        let mut matches =
            Vec::with_capacity(std::cmp::max(self.includes.len(), self.excludes.len()));

        self.exclude_globset()
            .matches_candidate_into(&cpath, &mut matches);
        for &i in matches.iter() {
            let ref exclude = self.excludes[i];
            if exclude.matches_file_type(file_type) {
                return None;
            }
        }

        self.include_globset()
            .matches_candidate_into(&cpath, &mut matches);
        for &i in matches.iter() {
            let ref include = self.includes[i];
            if include.matches_file_type(file_type) {
                return Some(include);
            }
        }

        None
    }
}

impl Default for Config {
    fn default() -> Self {
        Config::empty()
    }
}

impl PartialEq for Config {
    fn eq(&self, other: &Self) -> bool {
        if self.inherit_excludes != other.inherit_excludes {
            return false;
        }
        if self.inherit_includes != other.inherit_includes {
            return false;
        }
        if self.inherit_overrides != other.inherit_overrides {
            return false;
        }
        if self.includes != other.includes {
            return false;
        }
        if self.excludes != other.excludes {
            return false;
        }
        if self.overrides != other.overrides {
            return false;
        }
        true
    }
}

impl Eq for Config {}

#[derive(Debug)]
pub struct ConfigResolver {
    model_dir: PathBuf,
    configs: BTreeMap<PathBuf, Config>,
}

impl ConfigResolver {
    pub fn scan_revision(model_dir: &Path, commit_hash: &Sha1Hash) -> ModelResult<ConfigResolver> {
        let repo = Repository::open(model_dir).expect("Cannot open repository");
        let odb = repo.odb().expect("Cannot get git object database"); // FIXME ws error handling

        // FIXME ws error handling
        let obj = repo
            .find_object(commit_hash.as_oid(), None)
            .expect("cannot find object");
        let commit_tree = obj.peel_to_tree().expect("Non-tree oid found");

        let mut cr = ConfigResolver::new(&model_dir);

        commit_tree
            .walk(TreeWalkMode::PreOrder, |parent_path, entry| {
                if entry.kind() != Some(ObjectType::Blob)
                    || entry.name() != Some(DEFAULT_CONFIG_FILENAME)
                {
                    return TreeWalkResult::Ok;
                }
                // FIXME ws error handling
                let obj = odb.read(entry.id()).expect("Cannot get git object!");
                let content =
                    String::from_utf8(obj.data().to_vec()).expect("Config file is not valid utf8!");

                let config: Config = toml::from_str(&content).unwrap();
                cr.add_file(&model_dir.join(parent_path), config);

                TreeWalkResult::Ok
            })
            .expect("Error reading git tree"); // FIXME ws error handling

        let mut configs = BTreeMap::new();
        for path in cr.configs.keys() {
            let mut config = Config::standard();
            for (p, c) in cr.configs.iter() {
                if p.as_os_str().is_empty() || path.starts_with(p) {
                    if let Some(false) = c.inherit_excludes {
                        config.excludes.clear();
                    }
                    for e in c.excludes.iter() {
                        config.excludes.push(e.clone().with_base_path(p));
                    }
                    if let Some(false) = c.inherit_includes {
                        config.includes.clear();
                    }
                    for i in c.includes.iter() {
                        config.includes.push(i.clone().with_base_path(p));
                    }
                    if let Some(false) = c.inherit_overrides {
                        config.overrides.clear();
                    }
                    for (k, v) in c.overrides.iter() {
                        config.overrides.insert(k.clone(), v.clone());
                    }
                }
            }
            configs.insert(path.clone(), config);
        }

        if !configs.contains_key(Path::new("")) {
            configs.insert(PathBuf::new(), Config::standard());
        }

        cr.configs = configs;

        Ok(cr)
    }

    fn new(model_dir: &Path) -> ConfigResolver {
        debug_assert!(model_dir.is_absolute());

        ConfigResolver {
            model_dir: model_dir.to_path_buf(),
            configs: BTreeMap::new(),
        }
    }

    fn add_file(&mut self, path: &Path, config: Config) {
        debug_assert!(path.starts_with(&self.model_dir));

        let p = path.strip_prefix(&self.model_dir).unwrap();
        self.configs.insert(p.to_path_buf(), config);
    }

    pub fn resolve(&self, path: &Path) -> &Config {
        debug_assert!(path.starts_with(&self.model_dir));

        //        let path = if !path.is_dir() {
        //            path.parent().unwrap()
        //        } else {
        //            path
        //        };

        let path = path.strip_prefix(&self.model_dir).unwrap();

        for (p, c) in self.configs.iter().rev() {
            if p.as_os_str().is_empty() || path.starts_with(p) {
                return c;
            }
        }

        unreachable!();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PathBuf, &Config)> {
        self.configs.iter()
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    static CONFIG_STANDARD_TOML: &str = indoc!(r#"
    inherit_excludes = true
    inherit_includes = true
    inherit_overrides = true

    [[exclude]]
    path = "**/.*/**"

    [[include]]
    path = "**/*"
    file_type = "dir"
    item = "${map()}"
    mapping = "${$.find(array($item.@file_path_components[..-2]).join('.')).set($item.@file_name, $item)}"

    [[include]]
    path = "**/_.{yaml,yml,toml,json}"
    file_type = "file"
    item = "${loadFile(@.@file_path, @.@file_ext)}"
    mapping = "${$.find(array($item.@file_path_components[..-2]).join('.')).extend($item)}"

    [[include]]
    path = "**/*.{yaml,yml,toml,json}"
    file_type = "file"
    item = "${loadFile(@.@file_path, @.@file_ext)}"
    mapping = "${$.find(array($item.@file_path_components[..-2]).join('.')).set($item.@file_stem, $item)}"

    [[include]]
    path = "**/*"
    file_type = "file"
    item = "${loadFile(@.@file_path, 'text')}"
    mapping = "${$.find(array($item.@file_path_components[..-2]).join('.')).set($item.@file_stem, $item)}"

    [overrides]
    "#);

    #[test]
    fn standard_config_serialize() {
        let config = Config::standard();

        let toml = toml::to_string(&config).unwrap();
        assert_eq!(&toml, CONFIG_STANDARD_TOML);
    }

    #[test]
    fn standard_config_deserialize() {
        let config1 = Config::standard();
        let config2: Config = toml::from_str(CONFIG_STANDARD_TOML).unwrap();
        assert_eq!(&config1, &config2);
    }
}
