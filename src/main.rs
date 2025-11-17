use std::{
    collections::HashMap,
    fs,
    path::{Component, Path},
};

use clap::Parser;
use serde::Serialize;
use serde_json;
use serde_yaml;
use walkdir::WalkDir;

// Required CLI arguments
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

// Data types that can be used as an Option
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
enum OptionValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

// Holds a filepath and it's parsed YAML data
#[derive(Debug)]
struct FileData {
    path: String,
    data: HashMap<String, OptionValue>,
}

/// Validates and parses a YAML file containing Options
fn validate_and_parse(_group: &str, path: &str) -> Result<HashMap<String, OptionValue>, String> {
    let contents =
        fs::read_to_string(path).map_err(|e| format!("Failed to read file {}: {}", path, e))?;

    let data: HashMap<String, serde_yaml::Value> = serde_yaml::from_str(&contents)
        .map_err(|e| format!("Failed to parse YAML in {}: {}", path, e))?;

    let mut result = HashMap::new();

    // should only have one top level key named "options"
    if data.len() != 1 || !data.contains_key("options") {
        return Err(format!(
            "Expected one top level group named 'options', found {:?}",
            data.keys().collect::<Vec<_>>()
        ));
    }

    let options = data.get("options").unwrap();

    // options should be a Mapping
    // unwrap because we guarantee existence of key above
    if !options.is_mapping() {
        return Err(format!(
            "Expected 'options' to be a mapping, found {:?}",
            options
        ));
    }

    // unwrap because we guarantee it's a mapping
    for (option, option_value) in options.as_mapping().unwrap() {
        // TODO: verify option exists in schema
        // TODO: verify option value matches schema
        // TODO: verify option type is supported
        let value_parsed = match option_value {
            serde_yaml::Value::String(s) => OptionValue::String(s.clone()),
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    OptionValue::Int(i)
                } else if let Some(f) = n.as_f64() {
                    OptionValue::Float(f)
                } else {
                    return Err(format!(
                        "Unsupported number type for option {}: {}",
                        option.as_str().unwrap(),
                        n
                    ));
                }
            }
            serde_yaml::Value::Bool(b) => OptionValue::Bool(*b),
            _ => {
                return Err(format!(
                    "Unsupported value type for option {}: {:?}",
                    option.as_str().unwrap(),
                    option_value
                ));
            }
        };
        result.insert(option.as_str().unwrap().to_string(), value_parsed);
    }

    Ok(result)
}

/// Reads all YAML files in the root directory, validating and parsing them, then outputting
/// the options grouped by group and target
fn load(root: &str) -> Result<HashMap<String, HashMap<String, Vec<FileData>>>, String> {
    let mut grouped = HashMap::new();
    let root_path = Path::new(root);
    for entry in WalkDir::new(root) {
        match entry {
            Ok(dir_entry) => {
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
                        return Err(format!(
                            "Expected group/target/file.yaml, found {}",
                            relative_path.display()
                        ));
                    }

                    let group = parts[0];
                    let target = parts[1];
                    let fname = parts[2];

                    if fname.ends_with(".yml") {
                        return Err(format!(
                            "Expected {fname} to end with .yaml instead of .yml"
                        ));
                    }
                    // ignore non-yaml files
                    if !fname.ends_with(".yaml") {
                        continue;
                    }

                    // TODO: validate group name here
                    // if parts[0] not in group ...

                    let validated = validate_and_parse(group, &path_string)?;

                    let by_target = grouped
                        .entry(group.to_string())
                        .or_insert_with(HashMap::new)
                        .entry(target.to_string())
                        .or_insert_with(Vec::new);

                    by_target.push(FileData {
                        path: path_string,
                        data: validated,
                    })
                }
            }
            Err(e) => {
                return Err(format!("Error walking directory: {}", e));
            }
        }
    }

    // sort files for determinism
    for targets in grouped.values_mut() {
        for by_file in targets.values_mut() {
            by_file.sort_by(|a, b| a.path.cmp(&b.path));
        }
    }

    // TODO: Check overlap

    Ok(grouped)
}

/// Merges a set of option files into one
fn merge(filedata: &[FileData]) -> HashMap<String, OptionValue> {
    let mut result = HashMap::new();
    for FileData { data, .. } in filedata {
        for (key, value) in data {
            result.insert(key.clone(), value.clone());
        }
    }
    result
}

fn main() {
    // parse cli args
    let args = Args::parse();

    // TODO: Error if output directory already exists

    // load files
    let grouped = load(&args.root).unwrap();

    let mut json_outputs: Vec<(String, String)> = Vec::new();

    // merge files in the same target together
    for (group, targets) in grouped {
        let empty_vec = Vec::new();
        let defaults = merge(targets.get("default").unwrap_or(&empty_vec));

        // FIXME: The below is coded as Anthony did, where default is popped and is ignored in the loop.
        // If a group has only `default` as a target, we won't write any JSON?

        for (target, filedatas) in targets {
            if target == "default" {
                continue;
            }
            let mut merged = defaults.clone();
            merged.extend(merge(&filedatas));

            let mut with_option_key = HashMap::new();
            with_option_key.insert("options", merged);
            json_outputs.push((
                format!("sentry-options-{group}-{target}.json"),
                serde_json::to_string(&with_option_key).unwrap(),
            ));
        }
    }

    // write files
    fs::create_dir_all(&args.out).unwrap();
    for (filename, json_text) in json_outputs {
        let filepath = Path::new(&args.out).join(filename);
        fs::write(filepath, json_text).unwrap();
    }
}
