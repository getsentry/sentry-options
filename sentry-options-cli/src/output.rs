use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use clap::ValueEnum;
use serde::Serialize;

use crate::{AppError, FileData, NamespaceMap, OptionsMap, Result};

/// Output format for the write command
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum OutputFormat {
    #[default]
    Json,
    Configmap,
}

/// Kubernetes ConfigMap structure for YAML serialization
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigMap {
    api_version: String,
    kind: String,
    metadata: ConfigMapMetadata,
    data: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct ConfigMapMetadata {
    name: String,
    labels: BTreeMap<String, String>,
    annotations: BTreeMap<String, String>,
}

struct MergedOptions {
    namespace: String,
    target: String,
    options: BTreeMap<String, serde_json::Value>,
}

fn merge_keys(filedata: &[FileData]) -> OptionsMap {
    filedata
        .iter()
        .flat_map(|f| f.data.iter())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Merge options for a single namespace/target, applying default then target overrides.
fn merge_options_for_target(
    maps: &NamespaceMap,
    namespace: &str,
    target: &str,
) -> Result<BTreeMap<String, serde_json::Value>> {
    let targets = maps.get(namespace).ok_or_else(|| {
        AppError::Validation(format!("Namespace '{}' not found in values", namespace))
    })?;

    let default_files = targets.get("default").ok_or_else(|| {
        AppError::Validation(format!(
            "Namespace '{}' is missing required 'default' target",
            namespace
        ))
    })?;

    let mut merged = merge_keys(default_files);

    if target != "default" {
        let target_files = targets.get(target).ok_or_else(|| {
            AppError::Validation(format!(
                "Target '{}' not found in namespace '{}'",
                target, namespace
            ))
        })?;
        merged.extend(merge_keys(target_files));
    }

    Ok(merged.into_iter().collect())
}

fn merge_all_options(maps: NamespaceMap) -> Result<Vec<MergedOptions>> {
    let mut results = Vec::new();

    for (namespace, targets) in &maps {
        for target in targets.keys() {
            results.push(MergedOptions {
                namespace: namespace.clone(),
                target: target.clone(),
                options: merge_options_for_target(&maps, namespace, target)?,
            });
        }
    }

    results.sort_by(|a, b| (&a.namespace, &a.target).cmp(&(&b.namespace, &b.target)));
    Ok(results)
}

pub fn generate_json(maps: NamespaceMap) -> Result<Vec<(String, String)>> {
    merge_all_options(maps)?
        .into_iter()
        .map(|m| {
            let wrapper = BTreeMap::from([("options", m.options)]);
            Ok((
                format!("sentry-options-{}-{}.json", m.namespace, m.target),
                serde_json::to_string(&wrapper)?,
            ))
        })
        .collect()
}

pub fn generate_configmap(
    maps: &NamespaceMap,
    namespace: &str,
    target: &str,
    commit_sha: Option<&str>,
    commit_timestamp: Option<&str>,
    generated_at: &str,
) -> Result<ConfigMap> {
    let name = format!("sentry-options-{}-{}", namespace, target);

    let options = merge_options_for_target(maps, namespace, target)?;
    let wrapper = BTreeMap::from([("options", &options)]);
    let values_json = serde_json::to_string(&wrapper)?;

    let mut annotations = BTreeMap::from([("generated_at".to_string(), generated_at.to_string())]);
    if let Some(sha) = commit_sha {
        annotations.insert("commit_sha".to_string(), sha.to_string());
    }
    if let Some(ts) = commit_timestamp {
        annotations.insert("commit_timestamp".to_string(), ts.to_string());
    }

    Ok(ConfigMap {
        api_version: "v1".to_string(),
        kind: "ConfigMap".to_string(),
        metadata: ConfigMapMetadata {
            name,
            labels: BTreeMap::from([(
                "app.kubernetes.io/managed-by".to_string(),
                "sentry-options".to_string(),
            )]),
            annotations,
        },
        data: BTreeMap::from([("values.json".to_string(), values_json)]),
    })
}

pub fn write_configmap_yaml(configmap: &ConfigMap, out_path: Option<&Path>) -> Result<()> {
    let yaml = serde_yaml::to_string(configmap)
        .map_err(|e| AppError::Validation(format!("YAML serialization error: {}", e)))?;

    match out_path {
        Some(path) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, yaml)?;
        }
        None => {
            print!("{}", yaml);
        }
    }
    Ok(())
}

pub fn write_json(out_path: PathBuf, json_outputs: Vec<(String, String)>) -> Result<()> {
    fs::create_dir_all(&out_path)?;
    for (filename, json_text) in json_outputs {
        fs::write(out_path.join(&filename), json_text)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[allow(clippy::type_complexity)]
    fn make_namespace_map(
        entries: Vec<(&str, &str, Vec<(&str, serde_json::Value)>)>,
    ) -> NamespaceMap {
        let mut map: NamespaceMap = HashMap::new();
        for (namespace, target, options) in entries {
            map.entry(namespace.to_string())
                .or_default()
                .entry(target.to_string())
                .or_default()
                .push(FileData {
                    path: format!("{}/{}/test.yaml", namespace, target),
                    data: options
                        .into_iter()
                        .map(|(k, v)| (k.to_string(), v))
                        .collect(),
                });
        }
        map
    }

    #[test]
    fn test_generate_configmap() {
        let maps = make_namespace_map(vec![(
            "myns",
            "default",
            vec![
                ("string_val", serde_json::json!("hello")),
                ("int_val", serde_json::json!(42)),
            ],
        )]);

        let cm = generate_configmap(
            &maps,
            "myns",
            "default",
            Some("abc123"),
            Some("1705180800"),
            "2026-01-14T00:00:00Z",
        )
        .unwrap();

        assert_eq!(cm.api_version, "v1");
        assert_eq!(cm.kind, "ConfigMap");
        assert_eq!(cm.metadata.name, "sentry-options-myns-default");

        assert_eq!(cm.metadata.labels.len(), 1);
        assert_eq!(
            cm.metadata.labels.get("app.kubernetes.io/managed-by"),
            Some(&"sentry-options".to_string())
        );

        assert_eq!(
            cm.metadata.annotations.get("commit_sha"),
            Some(&"abc123".to_string())
        );
        assert_eq!(
            cm.metadata.annotations.get("commit_timestamp"),
            Some(&"1705180800".to_string())
        );
        assert_eq!(
            cm.metadata.annotations.get("generated_at"),
            Some(&"2026-01-14T00:00:00Z".to_string())
        );

        let values_json = cm.data.get("values.json").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(values_json).unwrap();
        assert_eq!(parsed["options"]["string_val"], "hello");
        assert_eq!(parsed["options"]["int_val"], 42);
    }

    #[test]
    fn test_generate_configmap_nonexistent_namespace() {
        let maps = make_namespace_map(vec![(
            "myns",
            "default",
            vec![("string_val", serde_json::json!("hello"))],
        )]);

        let result = generate_configmap(
            &maps,
            "nonexistent",
            "default",
            None,
            None,
            "2026-01-14T00:00:00Z",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_generate_configmap_nonexistent_target() {
        let maps = make_namespace_map(vec![(
            "myns",
            "default",
            vec![("string_val", serde_json::json!("hello"))],
        )]);

        let result = generate_configmap(
            &maps,
            "myns",
            "nonexistent",
            None,
            None,
            "2026-01-14T00:00:00Z",
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
