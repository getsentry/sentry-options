use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use walkdir::WalkDir;

use crate::{AppError, Result};

/// Loads all options from all YAML files under values_dir
/// returning a set of "namespace:option_name" pairs.
///
/// Intentionally simple, as we don't care about what the value is or which target it's from,
/// just that the value exists.
fn load_all_options(values_dir: &Path) -> Result<HashSet<String>> {
    let mut options = HashSet::new();

    for entry in WalkDir::new(values_dir) {
        let entry = entry?;

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        match path.extension().and_then(|ext| ext.to_str()) {
            Some("yaml") => {}
            _ => continue,
        }

        let path_string = path.display().to_string();

        // expects: values_dir/namespace/target/file.yaml
        let relative_path = path.strip_prefix(values_dir).map_err(|e| {
            AppError::Validation(format!(
                "Failed to get relative path for {}: {}",
                path_string, e
            ))
        })?;

        let namespace = relative_path
            .components()
            .next()
            .and_then(|c| c.as_os_str().to_str())
            .ok_or_else(|| {
                AppError::Validation(format!(
                    "{}: could not extract namespace from path",
                    path_string
                ))
            })?;

        // assume valid structure
        let file = fs::File::open(path)?;
        let data: HashMap<String, serde_yaml::Value> =
            serde_yaml::from_reader(file).map_err(|e| AppError::YamlParse {
                path: path_string.clone(),
                source: e,
            })?;

        // assume "options" key exists and is a mapping
        let yaml_options = data["options"]
            .as_mapping()
            .expect("expected 'options' to be a mapping");

        for key in yaml_options.keys() {
            if let Some(key_str) = key.as_str() {
                options.insert(format!("{}:{}", namespace, key_str));
            }
        }
    }

    Ok(options)
}

/// Checks if option values are still defined.
///
/// `deletions` is a space-separated list of namespace, option_name pairs delimited with a colon `:`.
/// e.g. "seer:feature.rate_limit seer:feature.enabled seer:feature.slugs" is a list of 3 options in the seer namespace.
///
/// `values_dir` is the directory containing all the namespaces, typically `sentry-options/values/`
///
/// Will return a list in the same format as the input, containing the options that still have value definitions.
///
/// Assumes that the values files are all well-formed.
pub fn check_option_usage(deletions: String, values_dir: &Path) -> Result<String> {
    let options_in_use = load_all_options(values_dir)?;

    let keys: Vec<&str> = deletions.split_whitespace().collect();

    // Validate key format
    for key in &keys {
        if !key.contains(':') {
            return Err(AppError::Validation(format!(
                "Invalid key format '{}', expected 'namespace:option'",
                key
            )));
        }
    }

    let result: Vec<&str> = keys
        .into_iter()
        .filter(|key| options_in_use.contains(*key))
        .collect();

    Ok(result.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_dir() -> TempDir {
        TempDir::new().unwrap()
    }

    fn create_values_yaml(dir: &Path, namespace: &str, target: &str, options: &[(&str, &str)]) {
        let target_dir = dir.join(namespace).join(target);
        fs::create_dir_all(&target_dir).unwrap();

        let options_map: HashMap<String, Value> = options
            .iter()
            .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
            .collect();

        let values = serde_json::json!({
            "options": options_map
        });

        let values_file = target_dir.join("values.yaml");
        fs::write(values_file, serde_yaml::to_string(&values).unwrap()).unwrap();
    }

    // Helper function for backwards compatibility with tests - creates values in default target
    fn create_values(dir: &Path, namespace: &str, options: &[(&str, &str)]) {
        create_values_yaml(dir, namespace, "default", options);
    }

    #[test]
    fn test_no_options_in_use() {
        let dir = setup_test_dir();
        create_values(
            dir.path(),
            "sentry",
            &[
                ("feature.enabled", "true"),
                ("system.url", "https://sentry.io"),
            ],
        );

        let deletions = "sentry:removed.option sentry:old.feature";
        let result = check_option_usage(deletions.to_string(), dir.path()).unwrap();

        assert_eq!(result, "");
    }

    #[test]
    fn test_some_options_in_use() {
        let dir = setup_test_dir();
        create_values(
            dir.path(),
            "sentry",
            &[
                ("feature.enabled", "true"),
                ("system.url", "https://sentry.io"),
                ("traces.sample-rate", "0.5"),
            ],
        );

        let deletions = "sentry:feature.enabled sentry:removed.option sentry:traces.sample-rate";
        let result = check_option_usage(deletions.to_string(), dir.path()).unwrap();

        assert_eq!(result, "sentry:feature.enabled sentry:traces.sample-rate");
    }

    #[test]
    fn test_all_options_in_use() {
        let dir = setup_test_dir();
        create_values(
            dir.path(),
            "sentry",
            &[
                ("feature.enabled", "true"),
                ("system.url", "https://sentry.io"),
            ],
        );

        let deletions = "sentry:feature.enabled sentry:system.url";
        let result = check_option_usage(deletions.to_string(), dir.path()).unwrap();

        assert_eq!(result, "sentry:feature.enabled sentry:system.url");
    }

    #[test]
    fn test_multiple_namespaces() {
        let dir = setup_test_dir();
        create_values(dir.path(), "sentry", &[("feature.enabled", "true")]);
        create_values(dir.path(), "relay", &[("relay.enabled", "true")]);
        create_values(dir.path(), "getsentry", &[("billing.enabled", "false")]);

        let deletions = "sentry:feature.enabled relay:relay.enabled relay:removed.option getsentry:billing.enabled";
        let result = check_option_usage(deletions.to_string(), dir.path()).unwrap();

        assert_eq!(
            result,
            "sentry:feature.enabled relay:relay.enabled getsentry:billing.enabled"
        );
    }

    #[test]
    fn test_namespace_not_exists() {
        let dir = setup_test_dir();

        let deletions = "nonexistent:some.option";
        let result = check_option_usage(deletions.to_string(), dir.path()).unwrap();

        // Namespace doesn't exist, so option can't be in use
        assert_eq!(result, "");
    }

    #[test]
    fn test_values_yaml_not_exists() {
        let dir = setup_test_dir();
        // Create namespace dir but no YAML files
        fs::create_dir_all(dir.path().join("sentry")).unwrap();

        let deletions = "sentry:some.option";
        let result = check_option_usage(deletions.to_string(), dir.path()).unwrap();

        assert_eq!(result, "");
    }

    #[test]
    fn test_empty_deletions() {
        let dir = setup_test_dir();
        create_values(dir.path(), "sentry", &[("feature.enabled", "true")]);

        let deletions = "";
        let result = check_option_usage(deletions.to_string(), dir.path()).unwrap();

        assert_eq!(result, "");
    }

    #[test]
    fn test_multiple_targets_and_files() {
        let dir = setup_test_dir();

        // Create multiple YAML files across different targets
        create_values_yaml(
            dir.path(),
            "sentry",
            "default",
            &[("feature.enabled", "true")],
        );
        create_values_yaml(dir.path(), "sentry", "s4s", &[("s4s.option", "value")]);

        // Create multiple files in the same target
        let default_dir = dir.path().join("sentry").join("default");
        let core_yaml = serde_json::json!({"options": {"core.option": "test"}});
        fs::write(
            default_dir.join("core.yaml"),
            serde_yaml::to_string(&core_yaml).unwrap(),
        )
        .unwrap();

        let deletions =
            "sentry:feature.enabled sentry:s4s.option sentry:core.option sentry:missing.option";
        let result = check_option_usage(deletions.to_string(), dir.path()).unwrap();

        // All three options across different files and targets should be found
        assert_eq!(
            result,
            "sentry:feature.enabled sentry:s4s.option sentry:core.option"
        );
    }
}
