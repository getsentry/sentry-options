use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Component, Path, PathBuf},
};

use clap::Parser;
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
#[derive(Debug, PartialEq, Eq)]
struct FileData {
    path: String,
    data: OptionsMap,
}

impl Ord for FileData {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.path.cmp(&other.path)
    }
}

impl PartialOrd for FileData {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
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

            let [namespace, target, fname] = parts.as_slice() else {
                return Err(AppError::Validation(format!(
                    "Invalid directory structure in {}: expected namespace/target/file.yaml",
                    relative_path.display()
                )));
            };

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

            // TODO: validate namespace name here
            // if namespace not in list_of_valid_namespaces ...

            let parsed_options = validate_and_parse(&path_string)?;

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

    // sort files for determinism
    for targets in grouped.values_mut() {
        for by_file in targets.values_mut() {
            by_file.sort();
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
                serde_json::to_string(&with_option_key)?,
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

    let grouped = load_and_validate(&args.root)?;

    ensure_no_duplicate_keys(&grouped)?;

    let json_outputs = generate_json(grouped)?;

    write_json(out_path, json_outputs)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// helper function to create a test file
    fn create_test_file(
        temp_dir: &TempDir,
        namespace: &str,
        target: &str,
        filename: &str,
        content: &str,
    ) -> PathBuf {
        let dir = temp_dir.path().join(namespace).join(target);
        fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join(filename);
        fs::write(&file_path, content).unwrap();
        file_path
    }

    /// helper function to build a yaml file
    fn valid_yaml(options: &[(&str, &str)]) -> String {
        let mut yaml = String::from("options:\n");
        for (key, value) in options {
            yaml.push_str(&format!("  {}: {}\n", key, value));
        }
        yaml
    }

    #[test]
    fn test_load_nonexistent_directory() {
        let result = load_and_validate("/foo/bar/baz");
        assert!(result.is_err());
        assert!(matches!(result, Err(AppError::Walk(_))));
    }

    #[test]
    fn test_invalid_directory_structure_too_few_levels() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("options.yaml");
        fs::write(&path, "options:\n  key: value").unwrap();

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("Invalid directory structure"));
                assert!(msg.contains("expected namespace/target/file.yaml"));
            }
            _ => panic!("Expected Validation error"),
        }
    }

    #[test]
    fn test_invalid_directory_structure_too_many_levels() {
        let temp_dir = TempDir::new().unwrap();
        let deep_dir = temp_dir
            .path()
            .join("namespace")
            .join("target")
            .join("extra_level")
            .join("level");
        fs::create_dir_all(&deep_dir).unwrap();
        fs::write(deep_dir.join("file.yaml"), "options:\n  key: value").unwrap();

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("Invalid directory structure"));
            }
            _ => panic!("Expected Validation error"),
        }
    }

    #[test]
    fn test_yml_extension_rejected() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "bad.yml",
            "options:\n  key: value",
        );

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("expected .yaml, found .yml"));
            }
            _ => panic!("Expected Validation error for .yml extension"),
        }
    }

    #[test]
    fn test_non_yaml_files_ignored() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(&temp_dir, "test", "default", "README.md", "# Documentation");
        create_test_file(&temp_dir, "test", "default", "config.txt", "some text");
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "valid.yaml",
            &valid_yaml(&[("key", "\"value\"")]),
        );

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_ok());
        let grouped = result.unwrap();
        assert_eq!(
            grouped.get("test").unwrap().get("default").unwrap().len(),
            1
        );
    }

    #[test]
    fn test_empty_yaml_file() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(&temp_dir, "test", "default", "empty.yaml", "");

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_err());
        match result {
            Err(AppError::YamlParse { .. }) | Err(AppError::Validation(_)) => {}
            _ => panic!("Expected YAML parse or validation error for empty file"),
        }
    }

    #[test]
    fn test_invalid_yaml_syntax() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "bad.yaml",
            "options:\n  key: [\n  invalid",
        );

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_err());
        assert!(matches!(result, Err(AppError::YamlParse { .. })));
    }

    #[test]
    fn test_yaml_missing_options_key() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "bad.yaml",
            "settings:\n  key: value",
        );

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("expected"));
                assert!(msg.contains("options"));
            }
            _ => panic!("Expected Validation error for missing 'options' key"),
        }
    }

    #[test]
    fn test_yaml_multiple_top_level_keys() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "bad.yaml",
            "options:\n  key: value\nextra:\n  other: value",
        );

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("exactly one top level key"));
            }
            _ => panic!("Expected Validation error for multiple top-level keys"),
        }
    }

    #[test]
    fn test_options_not_a_mapping() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(&temp_dir, "test", "default", "bad.yaml", "options: 12345");

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("expected 'options' to be a mapping"));
            }
            _ => panic!("Expected Validation error when options is not a mapping"),
        }
    }

    #[test]
    fn test_valid_single_namespace() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "base.yaml",
            &valid_yaml(&[("key1", "\"value1\""), ("key2", "42")]),
        );

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_ok());
        let grouped = result.unwrap();
        assert!(grouped.contains_key("test"));
        assert!(grouped.get("test").unwrap().contains_key("default"));
    }

    #[test]
    fn test_valid_multiple_namespaces() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "ns1",
            "default",
            "base.yaml",
            &valid_yaml(&[("key1", "\"value1\"")]),
        );
        create_test_file(
            &temp_dir,
            "ns2",
            "default",
            "base.yaml",
            &valid_yaml(&[("key2", "\"value2\"")]),
        );

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_ok());
        let grouped = result.unwrap();
        assert_eq!(grouped.len(), 2);
        assert!(grouped.contains_key("ns1"));
        assert!(grouped.contains_key("ns2"));
    }

    #[test]
    fn test_duplicate_keys_in_same_target() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "file1.yaml",
            &valid_yaml(&[("key1", "\"value1\"")]),
        );
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "file2.yaml",
            &valid_yaml(&[("key1", "\"value2\"")]),
        );

        let grouped = load_and_validate(temp_dir.path().to_str().unwrap()).unwrap();
        let result = ensure_no_duplicate_keys(&grouped);
        assert!(result.is_err());
        match result {
            Err(AppError::DuplicateKey { key, .. }) => {
                assert_eq!(key, "key1");
            }
            _ => panic!("Expected DuplicateKey error"),
        }
    }

    #[test]
    fn test_namespace_missing_default_target() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "test",
            "s4s",
            "base.yaml",
            &valid_yaml(&[("key1", "\"value1\"")]),
        );

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_err());
        match result {
            Err(AppError::Validation(msg)) => {
                assert!(msg.contains("missing required 'default' target"));
            }
            _ => panic!("Expected Validation error for missing default target"),
        }
    }

    #[test]
    fn test_multiple_files_in_target() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "file1.yaml",
            &valid_yaml(&[("key1", "\"value1\"")]),
        );
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "file2.yaml",
            &valid_yaml(&[("key2", "\"value2\"")]),
        );

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_ok());
        let grouped = result.unwrap();
        assert_eq!(
            grouped.get("test").unwrap().get("default").unwrap().len(),
            2
        );
    }

    #[test]
    fn test_target_override() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "base.yaml",
            &valid_yaml(&[("key1", "\"default_value\""), ("key2", "\"default2\"")]),
        );
        create_test_file(
            &temp_dir,
            "test",
            "s4s",
            "override.yaml",
            &valid_yaml(&[("key1", "\"overridden\"")]),
        );

        let grouped = load_and_validate(temp_dir.path().to_str().unwrap()).unwrap();
        let json_outputs = generate_json(grouped).unwrap();

        // Find the s4s output
        let s4s_output = json_outputs
            .iter()
            .find(|(name, _)| name.contains("s4s"))
            .unwrap();
        let json: serde_json::Value = serde_json::from_str(&s4s_output.1).unwrap();

        // Check that key1 was overridden
        assert_eq!(json["options"]["key1"].as_str().unwrap(), "overridden");
        // Check that key2 still has default value
        assert_eq!(json["options"]["key2"].as_str().unwrap(), "default2");
    }

    #[test]
    fn test_output_keys_sorted_alphabetically() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "base.yaml",
            &valid_yaml(&[("charlie", "1"), ("alpha", "2"), ("bravo", "3")]),
        );

        let grouped = load_and_validate(temp_dir.path().to_str().unwrap()).unwrap();
        let json_outputs = generate_json(grouped).unwrap();
        let json_str = &json_outputs[0].1;

        // Parse and check that keys are in alphabetical order
        let json: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let keys: Vec<&str> = json["options"]
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();

        assert_eq!(keys, vec!["alpha", "bravo", "charlie"]);
    }

    #[test]
    fn test_various_value_types() {
        let temp_dir = TempDir::new().unwrap();
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "base.yaml",
            r#"options:
  string_val: "hello"
  int_val: 42
  float_val: 7.77
  bool_val: true
"#,
        );

        let result = load_and_validate(temp_dir.path().to_str().unwrap());
        assert!(result.is_ok());

        let grouped = result.unwrap();
        let json_outputs = generate_json(grouped).unwrap();
        let json: serde_json::Value = serde_json::from_str(&json_outputs[0].1).unwrap();

        assert_eq!(json["options"]["string_val"], "hello");
        assert_eq!(json["options"]["int_val"], 42);
        assert_eq!(json["options"]["float_val"], 7.77);
        assert_eq!(json["options"]["bool_val"], true);
    }

    #[test]
    fn test_files_sorted_deterministically() {
        let temp_dir = TempDir::new().unwrap();
        // Create files in non-alphabetical order
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "z_file.yaml",
            &valid_yaml(&[("key1", "\"value1\"")]),
        );
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "a_file.yaml",
            &valid_yaml(&[("key2", "\"value2\"")]),
        );
        create_test_file(
            &temp_dir,
            "test",
            "default",
            "m_file.yaml",
            &valid_yaml(&[("key3", "\"value3\"")]),
        );

        // Load twice and ensure order is consistent
        let grouped1 = load_and_validate(temp_dir.path().to_str().unwrap()).unwrap();
        let grouped2 = load_and_validate(temp_dir.path().to_str().unwrap()).unwrap();

        let files1 = &grouped1.get("test").unwrap().get("default").unwrap();
        let files2 = &grouped2.get("test").unwrap().get("default").unwrap();

        // Check that file order is the same and sorted
        for (f1, f2) in files1.iter().zip(files2.iter()) {
            assert_eq!(f1.path, f2.path);
        }

        // Check alphabetical order
        for i in 0..files1.len() - 1 {
            assert!(files1[i].path < files1[i + 1].path);
        }
    }
}
