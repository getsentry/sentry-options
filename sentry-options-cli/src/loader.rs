use std::{
    collections::HashMap,
    fs,
    path::{Component, Path},
};

use sentry_options_validation::SchemaRegistry;
use walkdir::WalkDir;

use crate::{AppError, FileData, NamespaceMap, OptionsMap, Result};

/// Reads all YAML files in the root directory, validating and parsing them.
/// Then outputs options grouped by namespace and target.
/// Only performs file structure validation, e.g. path, suffix
pub fn load_and_validate(root: &str, schema_registry: &SchemaRegistry) -> Result<NamespaceMap> {
    let mut grouped = HashMap::new();
    let root_path = Path::new(root);
    for entry in WalkDir::new(root) {
        let dir_entry = entry?;

        // Only process files, skip directories
        if dir_entry.file_type().is_file() {
            let path = dir_entry.path();
            let path_string = path.display().to_string();
            // path relative to root
            let relative_path = path.strip_prefix(root_path).map_err(|e| {
                AppError::Validation(format!(
                    "Failed to get relative path for {}: {} (root: {})",
                    path.display(),
                    e,
                    root_path.display()
                ))
            })?;
            let parts: Vec<&str> = relative_path
                .components()
                .filter_map(|c| match c {
                    Component::Normal(s) => s.to_str(),
                    // ignore ., .., and other prefixes
                    _ => None,
                })
                .collect();

            let [namespace, target, fname]: [&str; 3] = parts.try_into().map_err(|_| {
                AppError::Validation(format!(
                    "Invalid directory structure in {}: expected namespace/target/file.yaml",
                    relative_path.display()
                ))
            })?;

            // validate namespace exists in schema registry
            if schema_registry.get(namespace).is_none() {
                return Err(AppError::Validation(format!(
                    "Unknown namespace '{}' in file {}. No schema found for this namespace.",
                    namespace, path_string
                )));
            }

            if fname.ends_with(".yml") {
                return Err(AppError::Validation(format!(
                    "Invalid file {}: expected .yaml, found .yml",
                    path_string
                )));
            }
            // ignore non-yaml files
            if !fname.ends_with(".yaml") {
                continue;
            }

            let parsed_options = validate_and_parse(&path_string, namespace, schema_registry)?;

            let by_target = grouped
                .entry(namespace.to_string())
                .or_insert_with(HashMap::new)
                .entry(target.to_string())
                .or_insert_with(Vec::new);

            by_target.push(FileData {
                path: path_string,
                data: parsed_options,
            })
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

    // sort files for determinism
    for targets in grouped.values_mut() {
        for by_file in targets.values_mut() {
            by_file.sort();
        }
    }

    Ok(grouped)
}

/// Validates and parses a YAML file containing Options.
/// Performs file content validation, including structure and typing.
fn validate_and_parse(
    path: &str,
    namespace: &str,
    schema_registry: &SchemaRegistry,
) -> Result<OptionsMap> {
    let file = fs::File::open(path)?;

    let data: HashMap<String, serde_yaml::Value> =
        serde_yaml::from_reader(file).map_err(|e| AppError::YamlParse {
            path: path.to_string(),
            source: e,
        })?;

    let mut result = HashMap::new();

    // should only have one top level key named "options"
    if data.len() != 1 {
        let keys: Vec<String> = data.keys().map(|k| k.to_string()).collect();
        return Err(AppError::Validation(format!(
            "Invalid YAML structure in {}: expected exactly one top level key 'options', found {:?}",
            path, keys
        )));
    }

    let Some(options) = data.get("options") else {
        let keys: Vec<String> = data.keys().map(|k| k.to_string()).collect();
        return Err(AppError::Validation(format!(
            "Invalid YAML structure in {}: expected top level key 'options', found {:?}",
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

    let values_json = serde_json::to_value(&result)?;
    schema_registry
        .validate_values(namespace, &values_json)
        .map_err(|e| AppError::Validation(format!("In file {}: {}", path, e)))?;

    Ok(result)
}

/// Checks options in the same target for duplicate keys
pub fn ensure_no_duplicate_keys(grouped: &NamespaceMap) -> Result<()> {
    for targets in grouped.values() {
        for filedata in targets.values() {
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
