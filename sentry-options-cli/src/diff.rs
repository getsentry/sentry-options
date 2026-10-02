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

/// The layer that supplies an effective value.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    /// The entry's own target sets the value in its files.
    Target,
    /// Inherited from the namespace's default target.
    Default,
    /// Nothing reaches the target from values, so the schema default applies.
    /// This includes regions with no target directory, as we don't even deploy
    /// the default target there.
    SchemaDefault,
    /// No value and no schema default (feature-flag keys).
    Unset,
}

/// An effective value and the layer that supplies it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResolvedValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    pub source: Source,
}

#[derive(Debug, Serialize)]
pub struct DiffEntry {
    pub namespace: String,
    pub target: String,
    pub key: String,
    pub before: ResolvedValue,
    pub after: ResolvedValue,
}

/// target -> the merged keys of that target directory alone (not layered with default)
type TargetMaps = HashMap<String, OptionsMap>;

/// same structure, but merges all files per target together
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

/// Essentially answers "what value would be returned for this namespace.target.key?"
fn resolve(
    namespace: Option<&TargetMaps>,
    schema: &NamespaceSchema,
    target: &str,
    key: &str,
) -> ResolvedValue {
    // skip this block if there is no namespace or target set
    if let Some(overrides) = namespace.and_then(|namespace| namespace.get(target)) {
        // has value?
        if let Some(value) = overrides.get(key) {
            return ResolvedValue {
                value: Some(value.clone()),
                source: Source::Target,
            };
        }
        // has value in default target?
        if let Some(value) = namespace
            .and_then(|namespace| namespace.get("default"))
            .and_then(|default| default.get(key))
        {
            return ResolvedValue {
                value: Some(value.clone()),
                source: Source::Default,
            };
        }
    }
    // has default value in schema?
    if let Some(value) = schema.get_default(key) {
        return ResolvedValue {
            value: Some(value.clone()),
            source: Source::SchemaDefault,
        };
    }
    // as far as we know, option doesn't exist
    ResolvedValue {
        value: None,
        source: Source::Unset,
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
    // unique set of namespaces between base and head
    let namespaces: BTreeSet<&String> = base
        .keys()
        .chain(head.keys())
        .filter(|namespace| !exclude_namespaces.contains(namespace))
        .collect();

    for namespace in namespaces {
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
    let base = loader::load_and_validate(&args.base, &registry)?;
    let head = loader::load_and_validate(&args.head, &registry)?;
    let entries = diff(&base, &head, &registry, &args.exclude_namespaces)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({ "changes": entries }))?
    );
    Ok(())
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
        let base = loader::load_and_validate(base_root.path().to_str().unwrap(), registry).unwrap();
        let head = loader::load_and_validate(head_root.path().to_str().unwrap(), registry).unwrap();
        diff(&base, &head, registry, &[]).unwrap()
    }

    fn entry<'a>(entries: &'a [DiffEntry], target: &str, key: &str) -> Option<&'a DiffEntry> {
        entries.iter().find(|e| e.target == target && e.key == key)
    }

    fn resolved(value: Option<serde_json::Value>, source: Source) -> ResolvedValue {
        ResolvedValue { value, source }
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
        assert_eq!(change.before, resolved(Some(1.0.into()), Source::Target));
        assert_eq!(change.after, resolved(Some(0.9.into()), Source::Target));
    }

    #[test]
    fn removed_target_override_falls_back_to_default() {
        let registry = registry(&["seer"]);
        let head = &[
            ("seer/default/values.yaml", "options:\n  rollout: 0.5\n"),
            ("seer/us/values.yaml", "options: {}\n"),
            ("seer/de/values.yaml", "options: {}\n"),
        ];
        let entries = run_diff(&registry, BASE, head);
        assert_eq!(entries.len(), 1);
        let change = entry(&entries, "us", "rollout").unwrap();
        assert_eq!(change.before, resolved(Some(1.0.into()), Source::Target));
        assert_eq!(change.after, resolved(Some(0.5.into()), Source::Default));
    }

    #[test]
    fn removed_default_falls_back_to_schema_default() {
        let registry = registry(&["seer"]);
        let head = &[
            ("seer/default/values.yaml", "options: {}\n"),
            ("seer/us/values.yaml", "options:\n  rollout: 1.0\n"),
            ("seer/de/values.yaml", "options: {}\n"),
        ];
        let entries = run_diff(&registry, BASE, head);
        assert_eq!(entries.len(), 1);
        let change = entry(&entries, "de", "rollout").unwrap();
        assert_eq!(change.before, resolved(Some(0.5.into()), Source::Default));
        assert_eq!(
            change.after,
            resolved(Some(0.0.into()), Source::SchemaDefault)
        );
        // us keeps its own override, so the default removal must not touch it
        assert!(entry(&entries, "us", "rollout").is_none());
    }

    #[test]
    fn no_value_and_no_schema_default_is_unset() {
        // feature-flag keys have no schema default; the loader accepts them,
        // so hand the maps to diff() directly
        let registry = registry(&["seer"]);
        let file = |data: &[(&str, serde_json::Value)]| {
            vec![crate::FileData {
                path: "seer/us/values.yaml".to_string(),
                data: data
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
            }]
        };
        let targets = |us: Vec<crate::FileData>| {
            HashMap::from([(
                "seer".to_string(),
                HashMap::from([("default".to_string(), file(&[])), ("us".to_string(), us)]),
            )])
        };
        let base = targets(file(&[("feature.x", true.into())]));
        let head = targets(file(&[]));
        let entries = diff(&base, &head, &registry, &[]).unwrap();
        let change = entry(&entries, "us", "feature.x").unwrap();
        assert_eq!(change.before, resolved(Some(true.into()), Source::Target));
        assert_eq!(change.after, resolved(None, Source::Unset));
    }

    #[test]
    fn missing_target_dir_resolves_to_schema_default_only() {
        let registry = registry(&["seer"]);
        let head = &[
            ("seer/default/values.yaml", "options:\n  rollout: 0.5\n"),
            ("seer/us/values.yaml", "options:\n  rollout: 1.0\n"),
            ("seer/de/values.yaml", "options: {}\n"),
            ("seer/s4s2/values.yaml", "options: {}\n"),
        ];
        let entries = run_diff(&registry, BASE, head);
        // without the s4s2 dir, s4s2 read pure schema defaults; now the
        // default target's values start shipping there
        let rollout = entry(&entries, "s4s2", "rollout").unwrap();
        assert_eq!(
            rollout.before,
            resolved(Some(0.0.into()), Source::SchemaDefault)
        );
        assert_eq!(rollout.after, resolved(Some(0.5.into()), Source::Default));
        // schema-default before and after: no entry
        assert!(entry(&entries, "s4s2", "enabled").is_none());
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
        let base =
            loader::load_and_validate(base_root.path().to_str().unwrap(), &registry).unwrap();
        let head =
            loader::load_and_validate(head_root.path().to_str().unwrap(), &registry).unwrap();
        let entries = diff(&base, &head, &registry, &["seer".to_string()]).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn sources_serialize_as_flat_strings() {
        let entry = DiffEntry {
            namespace: "seer".to_string(),
            target: "us".to_string(),
            key: "rollout".to_string(),
            before: resolved(None, Source::Unset),
            after: resolved(Some(0.0.into()), Source::SchemaDefault),
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["before"]["source"], "unset");
        assert!(json["before"].get("value").is_none());
        assert_eq!(json["after"]["source"], "schema-default");
        assert_eq!(json["after"]["value"], 0.0);
    }
}
