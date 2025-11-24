use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Component, Path, PathBuf},
};

use clap::Parser;
use serde_json;
use serde_yaml;
use walkdir::WalkDir;

/// Result type for operations
pub type Result<T> = std::result::Result<T, AppError>;

/// Errors that can occur during option processing
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Validation(String),

    #[error("Duplicate key '{key}' found in {first_file} and {second_file}")]
    DuplicateKey {
        key: String,
        first_file: String,
        second_file: String,
    },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("YAML parse error in {path}: {source}")]
    YamlParse {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Directory walk error: {0}")]
    Walk(#[from] walkdir::Error),
}

/// Required CLI arguments
#[derive(Parser, Debug)]
#[command(name = "sentry-options")]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, required = true, help = "root directory of the sentry options")]
    root: String,

    #[arg(long, required = true, help = "output directory for final json files")]
    out: String,
}

/// A key value pair of options and their parsed value
type OptionsMap = HashMap<String, serde_json::Value>;

/// Represents a filepath and its parsed YAML data
#[derive(Debug)]
struct FileData {
    path: String,
    data: OptionsMap,
}

/// A map representation of an option namespace
/// outer map is keyed by namespace
/// inner map is keyed by target, value a list of files
type NamespaceMap = HashMap<String, HashMap<String, Vec<FileData>>>;

/// Reads all YAML files in the root directory, validating and parsing them, then outputting
/// the options grouped by namespace and target
fn load_and_validate(root: &str) -> Result<NamespaceMap> {
    let mut grouped = HashMap::new();
    let root_path = Path::new(root);
    for entry in WalkDir::new(root) {
        let dir_entry = entry?;

        // Only process files, skip directories
        if dir_entry.file_type().is_file() {
            let path = dir_entry.path();
            let path_string = path.display().to_string();
            // path relative to root
            let relative_path = path.strip_prefix(root_path).unwrap_or(path);
            let parts: Vec<_> = relative_path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(s) => s.to_str(),
                    // ignore ., .., and other prefixes
                    _ => None,
                })
                .collect();

            let [namespace, target, fname] = parts.as_slice() else {
                return Err(AppError::Validation(format!(
                    "Invalid directory structure in {}: expected namespace/target/file.yaml",
                    relative_path.display()
                )));
            };

            if fname.ends_with(".yml") {
                return Err(AppError::Validation(format!(
                    "Invalid file {}: expected .yaml, found .yml",
                    path.display()
                )));
            }
            // ignore non-yaml files
            if !fname.ends_with(".yaml") {
                continue;
            }

            // TODO: validate namespace name here
            // if namespace not in list_of_valid_namespaces ...

            let validated = validate_and_parse(&path_string)?;

            let by_target = grouped
                .entry(namespace.to_string())
                .or_insert_with(HashMap::new)
                .entry(target.to_string())
                .or_insert_with(Vec::new);

            by_target.push(FileData {
                path: path_string,
                data: validated,
            })
        }
    }

    // sort files for determinism
    for targets in grouped.values_mut() {
        for by_file in targets.values_mut() {
            by_file.sort_by(|a, b| a.path.cmp(&b.path));
        }
    }

    // validate each namespace has a default target
    for (namespace, targets) in &grouped {
        if !targets.contains_key("default") {
            return Err(AppError::Validation(format!(
                "Namespace '{}' is missing required 'default' target",
                namespace
            )));
        }
    }

    Ok(grouped)
}

/// Validates and parses a YAML file containing Options
fn validate_and_parse(path: &str) -> Result<OptionsMap> {
    let file = fs::File::open(path)?;

    let data: HashMap<String, serde_yaml::Value> =
        serde_yaml::from_reader(file).map_err(|e| AppError::YamlParse {
            path: path.to_string(),
            source: e,
        })?;

    let mut result = HashMap::new();

    // should only have one top level key named "options"
    let Some(options) = data.get("options") else {
        let keys: Vec<String> = data.keys().map(|k| k.to_string()).collect();
        return Err(AppError::Validation(format!(
            "Invalid YAML structure in {}: expected one top level group named 'options', found {:?}",
            path, keys
        )));
    };

    // options should be a Mapping
    let Some(options_map) = options.as_mapping() else {
        return Err(AppError::Validation(format!(
            "Invalid YAML structure in {}: expected 'options' to be a mapping",
            path
        )));
    };

    for (option, option_value) in options_map {
        // TODO: verify option exists in schema
        // if option not in schema[namespace]

        // TODO: verify option value matches schema
        // if option.type == schema[namespace][target][option].type

        // Convert from serde_yaml::Value to serde_json::Value
        let json_value = serde_json::to_value(option_value)?;

        let option_key = option.as_str().ok_or_else(|| {
            AppError::Validation(format!(
                "Invalid YAML in {}: option key must be a string, found {:?}",
                path, option
            ))
        })?;

        result.insert(option_key.to_string(), json_value);
    }

    Ok(result)
}

/// Checks options in the same target for duplicate keys
fn ensure_no_duplicate_keys(grouped: &NamespaceMap) -> Result<()> {
    for (_, targets) in grouped {
        for (_, filedata) in targets {
            let mut key_to_file = HashMap::<String, String>::new();
            for FileData { path, data } in filedata {
                for key in data.keys() {
                    if let Some(first_file) = key_to_file.get(key) {
                        return Err(AppError::DuplicateKey {
                            key: key.to_string(),
                            first_file: first_file.to_string(),
                            second_file: path.to_string(),
                        });
                    }
                    key_to_file.insert(key.to_string(), path.to_string());
                }
            }
        }
    }
    Ok(())
}

/// Merges keys from many option files into one map
fn merge_keys(filedata: &[FileData]) -> OptionsMap {
    let mut result = HashMap::new();
    for FileData { data, .. } in filedata {
        for (key, value) in data {
            result.insert(key.clone(), value.clone());
        }
    }
    result
}

/// Generates the list of output JSON files
/// Uses the default target and handles overrides from other targets
fn generate_json(maps: NamespaceMap) -> Result<Vec<(String, String)>> {
    let mut json_outputs: Vec<(String, String)> = Vec::new();

    // merge files in the same target together
    for (namespace, targets) in maps {
        // This should never fail due to validation in load_and_validate,
        // but handle it gracefully just in case
        let Some(default_target) = targets.get("default") else {
            return Err(AppError::Validation(format!(
                "Namespace '{}' is missing required 'default' target",
                namespace
            )));
        };
        let defaults = merge_keys(default_target);

        for (target, filedatas) in targets {
            let mut merged = defaults.clone();
            merged.extend(merge_keys(&filedatas));

            // Convert to BTreeMap for sorted keys
            let sorted_merged: BTreeMap<_, _> = merged.into_iter().collect();

            let mut with_option_key = BTreeMap::new();
            with_option_key.insert("options", sorted_merged);
            json_outputs.push((
                format!("sentry-options-{namespace}-{target}.json"),
                serde_json::to_string_pretty(&with_option_key)?,
            ));
        }
    }
    Ok(json_outputs)
}

/// Writes JSON data to JSON files in the specified directory
fn write_json(out_path: PathBuf, json_outputs: Vec<(String, String)>) -> Result<()> {
    fs::create_dir_all(&out_path)?;

    for (filename, json_text) in json_outputs {
        let filepath = out_path.join(&filename);
        fs::write(&filepath, json_text)?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let out_path = PathBuf::from(&args.out);
    if out_path.exists() {
        return Err(AppError::Validation(format!(
            "Output directory already exists: {}",
            out_path.display()
        )));
    }

    let grouped = load_and_validate(&args.root)?;

    ensure_no_duplicate_keys(&grouped)?;

    let json_outputs = generate_json(grouped)?;

    write_json(out_path, json_outputs)?;

    Ok(())
}
