use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::Parser;
use serde::Serialize;
use serde_json;
use serde_yaml;
use walkdir::WalkDir;

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

// TODO: Add support for list, dict


// TODO: Can be removed, we validate later as well
/// Option types we support
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)] // don't output the type
enum OptionValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

/// A key value pair of options and their parsed value
type OptionsMap = HashMap<String, OptionValue>;

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
        let dir_entry = entry.context("Failed to read directory entry")?;

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

            if parts.len() != 3 {
                bail!(
                    "Invalid directory structure in {}: expected namespace/target/file.yaml",
                    relative_path.display()
                );
            }

            let namespace = parts[0];
            let target = parts[1];
            let fname = parts[2];

            if fname.ends_with(".yml") {
                bail!(
                    "Invalid file extension in {}: expected .yaml, found .yml",
                    path.display()
                );
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

    Ok(grouped)
}

/// Validates and parses a YAML file containing Options
fn validate_and_parse(path: &str) -> Result<OptionsMap> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("Failed to read file {}", path))?;

        // FIXME: from reader
    let data: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(&contents)
        .with_context(|| format!("Failed to parse YAML in {}", path))?;

    let mut result = HashMap::new();

    // should only have one top level key named "options"
    if data.len() != 1 || !data.contains_key("options") {
        let keys: Vec<String> = data.keys().map(|k| k.to_string()).collect();
        bail!(
            "Invalid YAML structure in {}: expected one top level group named 'options', found {:?}",
            path,
            keys
        );
    }

    let options = data.get("options").expect("key to be guaranteed above");

    // options should be a Mapping
    if !options.is_mapping() {
        bail!(
            "Invalid YAML structure in {}: expected 'options' to be a mapping",
            path
        );
    }

    for (option, option_value) in options
        .as_mapping()
        .expect("options guaranteed to be a map above")
    {
        // TODO: verify option exists in schema
        // if option not in schema[namespace]

        // TODO: verify option value matches schema
        // if option.type == schema[namespace][target][option].type

        let value_parsed = match option_value {
            serde_yaml::Value::String(s) => OptionValue::String(s.clone()),
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    OptionValue::Int(i)
                } else if let Some(f) = n.as_f64() {
                    OptionValue::Float(f)
                } else {
                    // theoretically impossible
                    let option_name = option.as_str().unwrap_or("unknown");
                    bail!(
                        "Unsupported value type in {} for option '{}': invalid number {}",
                        path,
                        option_name,
                        n
                    );
                }
            }
            serde_yaml::Value::Bool(b) => OptionValue::Bool(*b),
            _ => {
                let option_name = option.as_str().unwrap_or("unknown");
                bail!(
                    "Unsupported value type in {} for option '{}': {:?}",
                    path,
                    option_name,
                    option_value
                );
            }
        };
        result.insert(
            option.as_str().expect("option key to be valid").to_string(),
            value_parsed,
        );
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
                        bail!(
                            "Duplicate key '{}' found in {} and {}",
                            key,
                            first_file,
                            path
                        );
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
        let defaults = merge_keys(targets.get("default").unwrap_or(&Vec::new()));

        for (target, filedatas) in targets {
            let mut merged = defaults.clone();
            merged.extend(merge_keys(&filedatas));

            let mut with_option_key = HashMap::new();
            with_option_key.insert("options", merged);
            json_outputs.push((
                format!("sentry-options-{namespace}-{target}.json"),
                serde_json::to_string_pretty(&with_option_key)
                    .context("Failed to serialize JSON")?,
            ));
        }
    }
    Ok(json_outputs)
}

/// Writes JSON data to JSON files in the specified directory
fn write_json(out_path: PathBuf, json_outputs: Vec<(String, String)>) -> Result<()> {
    fs::create_dir_all(&out_path)
        .with_context(|| format!("Failed to create directory {}", out_path.display()))?;

    for (filename, json_text) in json_outputs {
        let filepath = out_path.join(&filename);
        fs::write(&filepath, json_text)
            .with_context(|| format!("Failed to write file {}", filepath.display()))?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();

    let out_path = PathBuf::from(&args.out);
    if out_path.exists() {
        bail!("Output directory already exists: {}", out_path.display());
    }

    let grouped = load_and_validate(&args.root)?;

    ensure_no_duplicate_keys(&grouped)?;

    let json_outputs = generate_json(grouped)?;

    write_json(out_path, json_outputs)?;

    Ok(())
}
