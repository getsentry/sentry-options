use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Write,
    path::PathBuf,
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

fn merge_all_options(maps: NamespaceMap) -> Result<Vec<MergedOptions>> {
    let mut results = Vec::new();

    for (namespace, targets) in maps {
        let default_target = targets.get("default").ok_or_else(|| {
            AppError::Validation(format!(
                "Namespace '{}' is missing required 'default' target",
                namespace
            ))
        })?;
        let defaults = merge_keys(default_target);

        for (target, filedatas) in targets {
            let mut merged = defaults.clone();
            merged.extend(merge_keys(&filedatas));

            results.push(MergedOptions {
                namespace: namespace.clone(),
                target,
                options: merged.into_iter().collect(),
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

/// Validate a Kubernetes ConfigMap name (DNS subdomain): lowercase alphanumeric,
/// '-', or '.', start/end with alphanumeric, max 253 characters.
fn validate_configmap_name(name: &str) -> Result<()> {
    if name.len() > 253 {
        return Err(AppError::Validation(format!(
            "ConfigMap name '{}' exceeds 253 character limit",
            name
        )));
    }
    if let Some(c) = name
        .chars()
        .find(|&c| !matches!(c, 'a'..='z' | '0'..='9' | '-' | '.'))
    {
        return Err(AppError::Validation(format!(
            "Invalid ConfigMap name '{}': invalid character '{}'. Use lowercase alphanumeric, '-', or '.'",
            name, c
        )));
    }
    if !name.starts_with(|c: char| c.is_ascii_alphanumeric())
        || !name.ends_with(|c: char| c.is_ascii_alphanumeric())
    {
        return Err(AppError::Validation(format!(
            "Invalid ConfigMap name '{}': must start and end with alphanumeric character",
            name
        )));
    }
    Ok(())
}

pub fn generate_configmaps(
    maps: NamespaceMap,
    commit_sha: Option<&str>,
    commit_timestamp: Option<&str>,
    generated_at: &str,
) -> Result<Vec<ConfigMap>> {
    merge_all_options(maps)?
        .into_iter()
        .map(|m| {
            let name = format!("sentry-options-{}-{}", m.namespace, m.target);
            validate_configmap_name(&name)?;

            let wrapper = BTreeMap::from([("options", &m.options)]);
            let values_json = serde_json::to_string(&wrapper)?;

            let mut annotations: BTreeMap<_, _> =
                [("sentry-options/generated-at", generated_at.to_string())].into();
            if let Some(sha) = commit_sha {
                annotations.insert("sentry-options/commit-sha", sha.to_string());
            }
            if let Some(ts) = commit_timestamp {
                annotations.insert("sentry-options/commit-timestamp", ts.to_string());
            }

            Ok(ConfigMap {
                api_version: "v1".to_string(),
                kind: "ConfigMap".to_string(),
                metadata: ConfigMapMetadata {
                    name,
                    labels: [(
                        "app.kubernetes.io/managed-by".to_string(),
                        "sentry-options".to_string(),
                    )]
                    .into(),
                    annotations: annotations
                        .into_iter()
                        .map(|(k, v)| (k.to_string(), v))
                        .collect(),
                },
                data: [("values.json".to_string(), values_json)].into(),
            })
        })
        .collect()
}

pub fn write_configmaps_yaml(configmaps: &[ConfigMap]) -> Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    for (i, cm) in configmaps.iter().enumerate() {
        if i > 0 {
            writeln!(handle, "---")?;
        }
        serde_yaml::to_writer(&mut handle, cm)
            .map_err(|e| AppError::Validation(format!("YAML serialization error: {}", e)))?;
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
    use super::*;

    #[test]
    fn test_validate_configmap_name_valid() {
        assert!(validate_configmap_name("sentry-options-relay-default").is_ok());
        assert!(validate_configmap_name("sentry-options-my.service-prod").is_ok());
        assert!(validate_configmap_name("a1-b2").is_ok());
    }

    #[test]
    fn test_validate_configmap_name_rejects_uppercase() {
        let result = validate_configmap_name("sentry-options-MyService-default");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid character")
        );
    }

    #[test]
    fn test_validate_configmap_name_rejects_underscore() {
        let result = validate_configmap_name("sentry-options-my_service-default");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid character")
        );
    }

    #[test]
    fn test_validate_configmap_name_rejects_leading_hyphen() {
        let result = validate_configmap_name("-sentry-options");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("start and end with alphanumeric")
        );
    }

    #[test]
    fn test_validate_configmap_name_rejects_trailing_hyphen() {
        let result = validate_configmap_name("sentry-options-");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("start and end with alphanumeric")
        );
    }

    #[test]
    fn test_validate_configmap_name_rejects_over_253_chars() {
        assert!(validate_configmap_name(&"a".repeat(253)).is_ok());
        let result = validate_configmap_name(&"a".repeat(254));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("exceeds 253 character limit")
        );
    }

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
    fn test_generate_configmaps() {
        let maps = make_namespace_map(vec![(
            "myns",
            "default",
            vec![
                ("string_val", serde_json::json!("hello")),
                ("int_val", serde_json::json!(42)),
            ],
        )]);

        let configmaps = generate_configmaps(
            maps,
            Some("abc123"),
            Some("1705180800"),
            "2026-01-14T00:00:00Z",
        )
        .unwrap();

        assert_eq!(configmaps.len(), 1);
        let cm = &configmaps[0];

        assert_eq!(cm.api_version, "v1");
        assert_eq!(cm.kind, "ConfigMap");
        assert_eq!(cm.metadata.name, "sentry-options-myns-default");

        assert_eq!(cm.metadata.labels.len(), 1);
        assert_eq!(
            cm.metadata.labels.get("app.kubernetes.io/managed-by"),
            Some(&"sentry-options".to_string())
        );

        assert_eq!(
            cm.metadata.annotations.get("sentry-options/commit-sha"),
            Some(&"abc123".to_string())
        );
        assert_eq!(
            cm.metadata
                .annotations
                .get("sentry-options/commit-timestamp"),
            Some(&"1705180800".to_string())
        );
        assert_eq!(
            cm.metadata.annotations.get("sentry-options/generated-at"),
            Some(&"2026-01-14T00:00:00Z".to_string())
        );

        let values_json = cm.data.get("values.json").unwrap();
        let parsed: serde_json::Value = serde_json::from_str(values_json).unwrap();
        assert_eq!(parsed["options"]["string_val"], "hello");
        assert_eq!(parsed["options"]["int_val"], 42);
    }

    #[test]
    fn test_generate_configmaps_sorted_deterministically() {
        let maps = make_namespace_map(vec![
            (
                "zns",
                "default",
                vec![("string_val", serde_json::json!("z"))],
            ),
            (
                "ans",
                "default",
                vec![("string_val", serde_json::json!("a"))],
            ),
        ]);

        let configmaps = generate_configmaps(maps, None, None, "2026-01-14T00:00:00Z").unwrap();

        assert_eq!(configmaps.len(), 2);
        assert_eq!(configmaps[0].metadata.name, "sentry-options-ans-default");
        assert_eq!(configmaps[1].metadata.name, "sentry-options-zns-default");
    }
}
