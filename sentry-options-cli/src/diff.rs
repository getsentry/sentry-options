use std::{
    collections::{BTreeSet, HashMap},
    path::Path,
};

use clap::Args;
use sentry_options_validation::{NamespaceSchema, SchemaRegistry};
use serde::Serialize;
use serde_json::Value;

use crate::{NamespaceMap, OptionsMap, Result, loader, output::merge_keys};

#[derive(Args, Debug)]
pub struct DiffArgs {
    #[arg(
        long,
        required = true,
        help = "directory containing namespace schema definitions"
    )]
    schemas: String,

    #[arg(long, required = true, help = "values root before the change")]
    base: String,

    #[arg(long, required = true, help = "values root after the change")]
    head: String,

    #[arg(
        long = "exclude-namespace",
        help = "namespace to leave out of the diff; repeatable"
    )]
    exclude_namespaces: Vec<String>,
}

/// An effective value and the layer that supplies it: the target's own files,
/// the namespace's default target, or the schema default. "not-deployed"
/// marks a target directory that doesn't exist on this side, "unset" a key
/// with no value and no schema default (feature flags).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    pub source: String,
}

#[derive(Debug, Serialize)]
pub struct DiffEntry {
    pub namespace: String,
    pub target: String,
    pub key: String,
    pub before: ResolvedValue,
    pub after: ResolvedValue,
}

/// target -> the merged keys of that target directory alone (not layered)
type TargetMaps = HashMap<String, OptionsMap>;

fn target_maps(map: &NamespaceMap) -> HashMap<String, TargetMaps> {
    map.iter()
        .map(|(namespace, targets)| {
            (
                namespace.clone(),
                targets
                    .iter()
                    .map(|(target, files)| (target.clone(), merge_keys(files)))
                    .collect(),
            )
        })
        .collect()
}

fn resolve(
    namespace: Option<&TargetMaps>,
    schema: &NamespaceSchema,
    target: &str,
    key: &str,
) -> ResolvedValue {
    let not_deployed = ResolvedValue {
        value: None,
        source: "not-deployed".to_string(),
    };
    let Some(namespace) = namespace else {
        return not_deployed;
    };
    let Some(pinned) = namespace.get(target) else {
        return not_deployed;
    };
    if let Some(value) = pinned.get(key) {
        return ResolvedValue {
            value: Some(value.clone()),
            source: target.to_string(),
        };
    }
    if let Some(value) = namespace
        .get("default")
        .and_then(|default| default.get(key))
    {
        return ResolvedValue {
            value: Some(value.clone()),
            source: "default".to_string(),
        };
    }
    if let Some(value) = schema.get_default(key) {
        return ResolvedValue {
            value: Some(value.clone()),
            source: "schema-default".to_string(),
        };
    }
    ResolvedValue {
        value: None,
        source: "unset".to_string(),
    }
}

/// Effective-value changes between two loaded values roots: every
/// (namespace, deployed target, key) whose resolved value or source layer
/// differs. The "default" target is the base layer, not a deployment, so it
/// surfaces through the targets it changes rather than as a row of its own.
pub fn diff(
    base: &NamespaceMap,
    head: &NamespaceMap,
    registry: &SchemaRegistry,
    exclude_namespaces: &[String],
) -> Result<Vec<DiffEntry>> {
    let base = target_maps(base);
    let head = target_maps(head);

    let mut entries = Vec::new();
    let namespaces: BTreeSet<&String> = base
        .keys()
        .chain(head.keys())
        .filter(|namespace| !exclude_namespaces.contains(namespace))
        .collect();

    for namespace in namespaces {
        // load_and_validate rejects namespaces without a schema
        let schema = registry
            .get(namespace)
            .expect("loaded namespace has a schema");
        let before_maps = base.get(namespace);
        let after_maps = head.get(namespace);

        let mut targets: BTreeSet<&String> = BTreeSet::new();
        for side in [before_maps, after_maps].into_iter().flatten() {
            targets.extend(side.keys().filter(|target| *target != "default"));
        }

        for target in targets {
            let mut keys: BTreeSet<&String> = schema.options.keys().collect();
            for side in [before_maps, after_maps].into_iter().flatten() {
                for layer in ["default", target.as_str()] {
                    if let Some(map) = side.get(layer) {
                        keys.extend(map.keys());
                    }
                }
            }

            for key in keys {
                let before = resolve(before_maps, schema, target, key);
                let after = resolve(after_maps, schema, target, key);
                if before != after {
                    entries.push(DiffEntry {
                        namespace: namespace.clone(),
                        target: target.clone(),
                        key: key.clone(),
                        before,
                        after,
                    });
                }
            }
        }
    }
    Ok(entries)
}

pub fn cli_diff(args: DiffArgs) -> Result<()> {
    let registry = SchemaRegistry::from_directory(Path::new(&args.schemas))?;
    let base = load_side(&args.base, &registry)?;
    let head = load_side(&args.head, &registry)?;
    let entries = diff(&base, &head, &registry, &args.exclude_namespaces)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({ "changes": entries }))?
    );
    Ok(())
}

fn load_side(root: &str, registry: &SchemaRegistry) -> Result<NamespaceMap> {
    let grouped = loader::load_and_validate(root, registry)?;
    loader::ensure_no_duplicate_keys(&grouped)?;
    Ok(grouped)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    const SCHEMA: &str = r#"{
        "version": "1.0",
        "type": "object",
        "properties": {
            "rollout": {"type": "number", "default": 0.0, "description": "test"},
            "enabled": {"type": "boolean", "default": false, "description": "test"}
        }
    }"#;

    fn registry(namespaces: &[&str]) -> SchemaRegistry {
        let schemas: Vec<(&str, &str)> = namespaces.iter().map(|ns| (*ns, SCHEMA)).collect();
        SchemaRegistry::from_schemas(&schemas).unwrap()
    }

    fn write_root(files: &[(&str, &str)]) -> TempDir {
        let root = TempDir::new().unwrap();
        for (path, content) in files {
            let path = root.path().join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        root
    }

    fn run_diff(
        registry: &SchemaRegistry,
        base: &[(&str, &str)],
        head: &[(&str, &str)],
    ) -> Vec<DiffEntry> {
        let base_root = write_root(base);
        let head_root = write_root(head);
        let base = load_side(base_root.path().to_str().unwrap(), registry).unwrap();
        let head = load_side(head_root.path().to_str().unwrap(), registry).unwrap();
        diff(&base, &head, registry, &[]).unwrap()
    }

    fn entry<'a>(entries: &'a [DiffEntry], target: &str, key: &str) -> Option<&'a DiffEntry> {
        entries.iter().find(|e| e.target == target && e.key == key)
    }

    fn resolved(value: Option<serde_json::Value>, source: &str) -> ResolvedValue {
        ResolvedValue {
            value,
            source: source.to_string(),
        }
    }

    const BASE: &[(&str, &str)] = &[
        ("seer/default/values.yaml", "options:\n  rollout: 0.5\n"),
        ("seer/us/values.yaml", "options:\n  rollout: 1.0\n"),
        ("seer/de/values.yaml", "options: {}\n"),
    ];

    #[test]
    fn identical_roots_have_no_changes() {
        let registry = registry(&["seer"]);
        assert!(run_diff(&registry, BASE, BASE).is_empty());
    }

    #[test]
    fn target_value_change_only_hits_that_target() {
        let registry = registry(&["seer"]);
        let head = &[
            ("seer/default/values.yaml", "options:\n  rollout: 0.5\n"),
            ("seer/us/values.yaml", "options:\n  rollout: 0.9\n"),
            ("seer/de/values.yaml", "options: {}\n"),
        ];
        let entries = run_diff(&registry, BASE, head);
        assert_eq!(entries.len(), 1);
        let change = entry(&entries, "us", "rollout").unwrap();
        assert_eq!(change.before, resolved(Some(1.0.into()), "us"));
        assert_eq!(change.after, resolved(Some(0.9.into()), "us"));
    }

    #[test]
    fn default_change_is_shadowed_by_target_pins() {
        let registry = registry(&["seer"]);
        let head = &[
            ("seer/default/values.yaml", "options:\n  rollout: 0.7\n"),
            ("seer/us/values.yaml", "options:\n  rollout: 1.0\n"),
            ("seer/de/values.yaml", "options: {}\n"),
        ];
        let entries = run_diff(&registry, BASE, head);
        assert_eq!(entries.len(), 1);
        let change = entry(&entries, "de", "rollout").unwrap();
        assert_eq!(change.before, resolved(Some(0.5.into()), "default"));
        assert_eq!(change.after, resolved(Some(0.7.into()), "default"));
    }

    #[test]
    fn removed_pin_falls_through_to_default_then_schema() {
        let registry = registry(&["seer"]);
        let head = &[
            ("seer/default/values.yaml", "options: {}\n"),
            ("seer/us/values.yaml", "options: {}\n"),
            ("seer/de/values.yaml", "options: {}\n"),
        ];
        let entries = run_diff(&registry, BASE, head);
        let us = entry(&entries, "us", "rollout").unwrap();
        assert_eq!(us.before, resolved(Some(1.0.into()), "us"));
        assert_eq!(us.after, resolved(Some(0.0.into()), "schema-default"));
        let de = entry(&entries, "de", "rollout").unwrap();
        assert_eq!(de.before, resolved(Some(0.5.into()), "default"));
        assert_eq!(de.after, resolved(Some(0.0.into()), "schema-default"));
    }

    #[test]
    fn source_only_change_is_reported() {
        let registry = registry(&["seer"]);
        let head = &[
            ("seer/default/values.yaml", "options:\n  rollout: 0.5\n"),
            ("seer/us/values.yaml", "options:\n  rollout: 1.0\n"),
            ("seer/de/values.yaml", "options:\n  rollout: 0.5\n"),
        ];
        let entries = run_diff(&registry, BASE, head);
        assert_eq!(entries.len(), 1);
        let change = entry(&entries, "de", "rollout").unwrap();
        assert_eq!(change.before, resolved(Some(0.5.into()), "default"));
        assert_eq!(change.after, resolved(Some(0.5.into()), "de"));
    }

    #[test]
    fn new_target_directory_starts_deploying() {
        let registry = registry(&["seer"]);
        let head = &[
            ("seer/default/values.yaml", "options:\n  rollout: 0.5\n"),
            ("seer/us/values.yaml", "options:\n  rollout: 1.0\n"),
            ("seer/de/values.yaml", "options: {}\n"),
            ("seer/s4s2/values.yaml", "options: {}\n"),
        ];
        let entries = run_diff(&registry, BASE, head);
        let rollout = entry(&entries, "s4s2", "rollout").unwrap();
        assert_eq!(rollout.before, resolved(None, "not-deployed"));
        assert_eq!(rollout.after, resolved(Some(0.5.into()), "default"));
        let enabled = entry(&entries, "s4s2", "enabled").unwrap();
        assert_eq!(
            enabled.after,
            resolved(Some(false.into()), "schema-default")
        );
    }

    #[test]
    fn excluded_namespace_is_skipped() {
        let registry = registry(&["seer"]);
        let head = &[
            ("seer/default/values.yaml", "options:\n  rollout: 0.5\n"),
            ("seer/us/values.yaml", "options:\n  rollout: 0.9\n"),
            ("seer/de/values.yaml", "options: {}\n"),
        ];
        let base_root = write_root(BASE);
        let head_root = write_root(head);
        let base = load_side(base_root.path().to_str().unwrap(), &registry).unwrap();
        let head = load_side(head_root.path().to_str().unwrap(), &registry).unwrap();
        let entries = diff(&base, &head, &registry, &["seer".to_string()]).unwrap();
        assert!(entries.is_empty());
    }
}
