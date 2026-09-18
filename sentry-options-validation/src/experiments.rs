use std::collections::{BTreeMap, HashMap};
use std::fmt;

use serde::Deserialize;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

pub const EXPERIMENT_LAYER_KEY_PREFIX: &str = "experiment-layer.";
pub const LAYER_SLOTS: u32 = 100;
pub const DEFAULT_ALLOCATION_SIZE: u32 = 20;
pub const MAX_ARMS: usize = 10;
pub const MAX_ARM_WEIGHT: u64 = 1_000_000_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Arm {
    pub name: String,
    pub weight: u64,
    #[serde(default)]
    pub config: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct Allocation {
    pub start: u32,
    #[serde(default = "default_size")]
    pub size: u32,
}

fn default_size() -> u32 {
    DEFAULT_ALLOCATION_SIZE
}

impl Allocation {
    pub fn end(&self) -> u32 {
        self.start.saturating_add(self.size)
    }

    pub fn contains(&self, slot: u32) -> bool {
        self.start <= slot && slot < self.end()
    }

    pub fn overlaps(&self, other: &Allocation) -> bool {
        self.start < other.end() && other.start < self.end()
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExperimentDefinition {
    #[serde(default)]
    pub layer: String,
    #[serde(default)]
    pub unit: Vec<String>,
    pub allocation: Allocation,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub arms: Vec<Arm>,
}

fn default_enabled() -> bool {
    true
}

impl ExperimentDefinition {
    pub fn from_value(value: &Value) -> Result<Self, String> {
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())
    }

    pub fn total_weight(&self) -> u64 {
        self.arms
            .iter()
            .map(|arm| arm.weight)
            .fold(0, u64::saturating_add)
    }

    pub fn revision(&self) -> String {
        let arms: Vec<Value> = self
            .arms
            .iter()
            .map(|arm| json!({"name": arm.name, "weight": arm.weight, "config": arm.config}))
            .collect();
        let payload = json!({
            "layer": self.layer,
            "unit": self.unit,
            "allocation": {"start": self.allocation.start, "size": self.allocation.size},
            "enabled": self.enabled,
            "arms": arms,
        });
        let serialized = serde_json::to_string(&payload).expect("payload serializes");
        let digest = Sha1::digest(serialized.as_bytes());
        digest[..8].iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LayerDefinition {
    pub unit: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
    pub experiments: BTreeMap<String, ExperimentDefinition>,
}

impl LayerDefinition {
    pub fn from_value(value: &Value) -> Result<Self, String> {
        serde_json::from_value(value.clone()).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerMember {
    pub experiment: String,
    pub allocation: Allocation,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExperimentIssue {
    InvalidDefinition {
        key: String,
        message: String,
    },
    AllocationOutOfRange {
        key: String,
        start: u32,
        size: u32,
    },
    TooManyArms {
        key: String,
        count: usize,
    },
    ZeroTotalWeight {
        key: String,
    },
    DuplicateArm {
        key: String,
        arm: String,
    },
    ArmWeightTooLarge {
        key: String,
        arm: String,
        weight: u64,
    },
    Overlap {
        layer: String,
        first: String,
        second: String,
        first_alloc: Allocation,
        second_alloc: Allocation,
        free: String,
    },
    DuplicateExperiment {
        experiment: String,
        first_layer: String,
        second_layer: String,
    },
}

impl fmt::Display for ExperimentIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = EXPERIMENT_LAYER_KEY_PREFIX;
        match self {
            Self::InvalidDefinition { key, message } => write!(f, "{key}: {message}"),
            Self::AllocationOutOfRange { key, size: 0, .. } => {
                write!(f, "{key}: allocation size must be at least 1")
            }
            Self::AllocationOutOfRange { key, start, size } => write!(
                f,
                "{key}: allocation start {start} + size {size} runs past the last slot ({}); shrink size or move start",
                LAYER_SLOTS - 1
            ),
            Self::TooManyArms { key, count } => write!(
                f,
                "{key}: {count} arms exceeds the maximum of {MAX_ARMS}; remove arms until at most {MAX_ARMS} remain"
            ),
            Self::ZeroTotalWeight { key } => write!(
                f,
                "{key}: arm weights sum to 0; give at least one arm a weight, or set enabled: false to pause"
            ),
            Self::DuplicateArm { key, arm } => write!(f, "{key}: arm '{arm}' is declared twice"),
            Self::ArmWeightTooLarge { key, arm, weight } => write!(
                f,
                "{key}: arm '{arm}' weight {weight} exceeds the maximum of {MAX_ARM_WEIGHT}; lower it to at most {MAX_ARM_WEIGHT}"
            ),
            Self::Overlap {
                layer,
                first,
                second,
                first_alloc,
                second_alloc,
                free,
            } => write!(
                f,
                "{prefix}{layer}: {second} ({}-{}) overlaps {first} ({}-{}); free slots: {free}",
                second_alloc.start,
                second_alloc.end() - 1,
                first_alloc.start,
                first_alloc.end() - 1
            ),
            Self::DuplicateExperiment {
                experiment,
                first_layer,
                second_layer,
            } => write!(
                f,
                "experiment '{experiment}' is declared in both {prefix}{first_layer} and {prefix}{second_layer}; an experiment name belongs to one layer"
            ),
        }
    }
}

pub fn issues_message(issues: &[ExperimentIssue]) -> String {
    issues.iter().map(|issue| format!("\n\t{issue}")).collect()
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExperimentSet {
    experiments: HashMap<String, ExperimentDefinition>,
    layers: HashMap<String, Vec<LayerMember>>,
}

impl ExperimentSet {
    pub fn from_values<'a, I>(values: I) -> Result<Self, Vec<ExperimentIssue>>
    where
        I: IntoIterator<Item = (&'a String, &'a Value)>,
    {
        let entries: BTreeMap<&str, &Value> = values
            .into_iter()
            .filter_map(|(key, value)| {
                key.strip_prefix(EXPERIMENT_LAYER_KEY_PREFIX)
                    .map(|name| (name, value))
            })
            .collect();

        let mut issues = Vec::new();
        let mut experiments: HashMap<String, ExperimentDefinition> = HashMap::new();
        let mut first_layer_of: HashMap<String, String> = HashMap::new();
        let mut layers: HashMap<String, Vec<LayerMember>> = HashMap::new();

        for (layer_name, value) in entries {
            let layer_key = format!("{EXPERIMENT_LAYER_KEY_PREFIX}{layer_name}");
            let layer = match LayerDefinition::from_value(value) {
                Ok(layer) => layer,
                Err(message) => {
                    issues.push(ExperimentIssue::InvalidDefinition {
                        key: layer_key,
                        message,
                    });
                    continue;
                }
            };
            for (name, mut def) in layer.experiments {
                def.layer = layer_name.to_string();
                def.unit = layer.unit.clone();
                let key = format!("{layer_key}: {name}");
                check_definition(&key, &def, &mut issues);
                match first_layer_of.get(&name) {
                    Some(first) if first != layer_name => {
                        issues.push(ExperimentIssue::DuplicateExperiment {
                            experiment: name.clone(),
                            first_layer: first.clone(),
                            second_layer: layer_name.to_string(),
                        });
                    }
                    Some(_) => {}
                    None => {
                        first_layer_of.insert(name.clone(), layer_name.to_string());
                    }
                }
                layers
                    .entry(layer_name.to_string())
                    .or_default()
                    .push(LayerMember {
                        experiment: name.clone(),
                        allocation: def.allocation,
                        enabled: def.enabled,
                    });
                experiments.insert(name, def);
            }
        }

        for members in layers.values_mut() {
            members.sort_by(|a, b| {
                (a.allocation.start, &a.experiment).cmp(&(b.allocation.start, &b.experiment))
            });
        }

        let mut layer_names: Vec<&String> = layers.keys().collect();
        layer_names.sort();
        for layer in layer_names {
            let members = layers.get(layer).expect("layer was just inserted");
            let free = free_slots(members);
            for pair in members.windows(2) {
                let (first, second) = (&pair[0], &pair[1]);
                if first.allocation.overlaps(&second.allocation) {
                    issues.push(ExperimentIssue::Overlap {
                        layer: layer.clone(),
                        first: first.experiment.clone(),
                        second: second.experiment.clone(),
                        first_alloc: first.allocation,
                        second_alloc: second.allocation,
                        free: free.clone(),
                    });
                }
            }
        }

        if issues.is_empty() {
            Ok(Self {
                experiments,
                layers,
            })
        } else {
            Err(issues)
        }
    }

    pub fn get(&self, name: &str) -> Option<&ExperimentDefinition> {
        self.experiments.get(name)
    }

    pub fn layer(&self, layer: &str) -> &[LayerMember] {
        self.layers.get(layer).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn owner_of(&self, layer: &str, slot: u32) -> Option<&LayerMember> {
        self.layer(layer)
            .iter()
            .find(|member| member.allocation.contains(slot))
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        let mut names: Vec<&str> = self.experiments.keys().map(String::as_str).collect();
        names.sort_unstable();
        names.into_iter()
    }
}

fn free_slots(members: &[LayerMember]) -> String {
    let mut claimed = [false; LAYER_SLOTS as usize];
    for member in members {
        let end = member.allocation.end().min(LAYER_SLOTS);
        for slot in member.allocation.start.min(LAYER_SLOTS)..end {
            claimed[slot as usize] = true;
        }
    }
    let mut ranges: Vec<String> = Vec::new();
    let mut slot = 0u32;
    while slot < LAYER_SLOTS {
        if claimed[slot as usize] {
            slot += 1;
            continue;
        }
        let start = slot;
        while slot < LAYER_SLOTS && !claimed[slot as usize] {
            slot += 1;
        }
        ranges.push(format!("{start}-{}", slot - 1));
    }
    if ranges.is_empty() {
        "none".to_string()
    } else {
        ranges.join(", ")
    }
}

fn check_definition(key: &str, def: &ExperimentDefinition, issues: &mut Vec<ExperimentIssue>) {
    if def.allocation.size == 0 || def.allocation.end() > LAYER_SLOTS {
        issues.push(ExperimentIssue::AllocationOutOfRange {
            key: key.to_string(),
            start: def.allocation.start,
            size: def.allocation.size,
        });
    }
    if def.arms.len() > MAX_ARMS {
        issues.push(ExperimentIssue::TooManyArms {
            key: key.to_string(),
            count: def.arms.len(),
        });
    }
    let mut seen = std::collections::HashSet::new();
    for arm in &def.arms {
        if !seen.insert(arm.name.as_str()) {
            issues.push(ExperimentIssue::DuplicateArm {
                key: key.to_string(),
                arm: arm.name.clone(),
            });
        }
        if arm.weight > MAX_ARM_WEIGHT {
            issues.push(ExperimentIssue::ArmWeightTooLarge {
                key: key.to_string(),
                arm: arm.name.clone(),
                weight: arm.weight,
            });
        }
    }
    if def.enabled && def.total_weight() == 0 {
        issues.push(ExperimentIssue::ZeroTotalWeight {
            key: key.to_string(),
        });
    }
}

pub fn validate_experiments(values: &Value) -> Vec<ExperimentIssue> {
    match values.as_object() {
        Some(map) => ExperimentSet::from_values(map.iter())
            .err()
            .unwrap_or_default(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn experiment(start: u32, size: u32) -> Value {
        json!({
            "owner": {"team": "testing"},
            "allocation": {"start": start, "size": size},
            "arms": [{"name": "control", "weight": 50}, {"name": "treatment", "weight": 50}]
        })
    }

    fn layer(unit: &str, experiments: Vec<(&str, Value)>) -> Value {
        let exps: serde_json::Map<String, Value> = experiments
            .into_iter()
            .map(|(name, value)| (name.to_string(), value))
            .collect();
        json!({"unit": [unit], "experiments": exps})
    }

    fn set_from(entries: Vec<(&str, Value)>) -> Result<ExperimentSet, Vec<ExperimentIssue>> {
        let map: serde_json::Map<String, Value> = entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ExperimentSet::from_values(map.iter())
    }

    fn def_with_layer(layer_name: &str, start: u32, size: u32) -> ExperimentDefinition {
        let mut def = ExperimentDefinition::from_value(&experiment(start, size)).unwrap();
        def.layer = layer_name.to_string();
        def.unit = vec!["organization_id".to_string()];
        def
    }

    #[test]
    fn revision_is_stable_across_builds() {
        let a = def_with_layer("checkout", 0, 40);
        let b = def_with_layer("checkout", 0, 40);
        assert_eq!(a.revision(), b.revision());
        assert_eq!(a.revision().len(), 16);
    }

    #[test]
    fn revision_changes_with_arm_config() {
        let base = def_with_layer("checkout", 0, 40);
        let mut v = experiment(0, 40);
        v["arms"][1]["config"] = json!({"color": "green"});
        let mut changed = ExperimentDefinition::from_value(&v).unwrap();
        changed.layer = "checkout".to_string();
        changed.unit = vec!["organization_id".to_string()];
        assert_ne!(base.revision(), changed.revision());
    }

    #[test]
    fn revision_changes_with_weight() {
        let base = def_with_layer("checkout", 0, 40);
        let mut v = experiment(0, 40);
        v["arms"][0]["weight"] = json!(70);
        let mut changed = ExperimentDefinition::from_value(&v).unwrap();
        changed.layer = "checkout".to_string();
        changed.unit = vec!["organization_id".to_string()];
        assert_ne!(base.revision(), changed.revision());
    }

    #[test]
    fn revision_ignores_owner_and_description() {
        let base = def_with_layer("checkout", 0, 40);
        let mut v = experiment(0, 40);
        v["owner"] = json!({"team": "someone-else"});
        v["description"] = json!("a different description");
        v["created_at"] = json!("2030-01-01");
        let mut same = ExperimentDefinition::from_value(&v).unwrap();
        same.layer = "checkout".to_string();
        same.unit = vec!["organization_id".to_string()];
        assert_eq!(base.revision(), same.revision());
    }

    #[test]
    fn parses_definition_with_defaults() {
        let def = ExperimentDefinition::from_value(&experiment(0, 40)).unwrap();
        assert!(def.enabled);
        assert_eq!(def.total_weight(), 100);
        assert_eq!(def.allocation, Allocation { start: 0, size: 40 });
        assert_eq!(def.arms[1].config, None);
    }

    #[test]
    fn allocation_size_defaults_to_twenty() {
        let value = json!({
            "owner": {"team": "testing"},
            "allocation": {"start": 0},
            "arms": [{"name": "control", "weight": 1}]
        });
        let def = ExperimentDefinition::from_value(&value).unwrap();
        assert_eq!(def.allocation, Allocation { start: 0, size: 20 });
    }

    #[test]
    fn ignores_keys_without_prefix() {
        let set = set_from(vec![
            ("int-option", json!(3)),
            (
                "experiment-layer.l",
                layer("organization_id", vec![("a", experiment(0, 10))]),
            ),
        ])
        .unwrap();
        assert_eq!(set.names().collect::<Vec<_>>(), vec!["a"]);
    }

    #[test]
    fn layer_members_sorted_by_start_and_owner_lookup() {
        let set = set_from(vec![(
            "experiment-layer.l",
            layer(
                "organization_id",
                vec![("b", experiment(40, 30)), ("a", experiment(0, 40))],
            ),
        )])
        .unwrap();
        let members: Vec<&str> = set
            .layer("l")
            .iter()
            .map(|m| m.experiment.as_str())
            .collect();
        assert_eq!(members, vec!["a", "b"]);
        assert_eq!(set.owner_of("l", 39).unwrap().experiment, "a");
        assert_eq!(set.owner_of("l", 40).unwrap().experiment, "b");
        assert_eq!(set.owner_of("l", 69).unwrap().experiment, "b");
        assert!(set.owner_of("l", 70).is_none());
        assert!(set.layer("other").is_empty());
    }

    #[test]
    fn rejects_overlapping_allocations() {
        let err = set_from(vec![(
            "experiment-layer.l",
            layer(
                "organization_id",
                vec![("a", experiment(0, 50)), ("b", experiment(49, 10))],
            ),
        )])
        .unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(matches!(
            &err[0],
            ExperimentIssue::Overlap { layer, first, second, .. }
                if layer == "l" && first == "a" && second == "b"
        ));
    }

    #[test]
    fn overlap_message_lists_free_slots() {
        let err = set_from(vec![(
            "experiment-layer.checkout",
            layer(
                "organization_id",
                vec![
                    ("checkout-color", experiment(0, 50)),
                    ("checkout-copy", experiment(30, 30)),
                ],
            ),
        )])
        .unwrap_err();
        assert_eq!(
            err[0].to_string(),
            "experiment-layer.checkout: checkout-copy (30-59) overlaps checkout-color (0-49); free slots: 60-99"
        );
    }

    #[test]
    fn overlap_message_says_none_when_layer_is_full() {
        let err = set_from(vec![(
            "experiment-layer.l",
            layer(
                "organization_id",
                vec![("a", experiment(0, 100)), ("b", experiment(90, 10))],
            ),
        )])
        .unwrap_err();
        assert!(
            err[0].to_string().ends_with("free slots: none"),
            "{}",
            err[0]
        );
    }

    #[test]
    fn adjacent_allocations_do_not_overlap() {
        assert!(
            set_from(vec![(
                "experiment-layer.l",
                layer(
                    "organization_id",
                    vec![("a", experiment(0, 50)), ("b", experiment(50, 50))],
                ),
            )])
            .is_ok()
        );
    }

    #[test]
    fn same_allocation_in_different_layers_is_fine() {
        assert!(
            set_from(vec![
                (
                    "experiment-layer.l1",
                    layer("organization_id", vec![("a", experiment(0, 100))]),
                ),
                (
                    "experiment-layer.l2",
                    layer("organization_id", vec![("b", experiment(0, 100))]),
                ),
            ])
            .is_ok()
        );
    }

    #[test]
    fn rejects_allocation_past_last_slot() {
        let err = set_from(vec![(
            "experiment-layer.l",
            layer("organization_id", vec![("a", experiment(90, 20))]),
        )])
        .unwrap_err();
        assert_eq!(
            err,
            vec![ExperimentIssue::AllocationOutOfRange {
                key: "experiment-layer.l: a".into(),
                start: 90,
                size: 20
            }]
        );
    }

    #[test]
    fn accepts_allocation_of_one_slot() {
        assert!(
            set_from(vec![(
                "experiment-layer.l",
                layer("organization_id", vec![("a", experiment(0, 1))]),
            )])
            .is_ok()
        );
    }

    #[test]
    fn rejects_zero_size_allocation() {
        let err = set_from(vec![(
            "experiment-layer.l",
            layer("organization_id", vec![("a", experiment(0, 0))]),
        )])
        .unwrap_err();
        assert_eq!(
            err,
            vec![ExperimentIssue::AllocationOutOfRange {
                key: "experiment-layer.l: a".into(),
                start: 0,
                size: 0
            }]
        );
        assert_eq!(
            err[0].to_string(),
            "experiment-layer.l: a: allocation size must be at least 1"
        );
    }

    fn experiment_with_arms(count: usize) -> Value {
        let arms: Vec<Value> = (0..count)
            .map(|i| json!({"name": format!("arm{i}"), "weight": 1}))
            .collect();
        let mut v = experiment(0, 10);
        v["arms"] = json!(arms);
        v
    }

    #[test]
    fn rejects_more_arms_than_maximum() {
        let err = set_from(vec![(
            "experiment-layer.l",
            layer("organization_id", vec![("a", experiment_with_arms(11))]),
        )])
        .unwrap_err();
        assert_eq!(
            err,
            vec![ExperimentIssue::TooManyArms {
                key: "experiment-layer.l: a".into(),
                count: 11
            }]
        );
    }

    #[test]
    fn rejects_arm_weight_over_the_maximum() {
        let mut over = experiment(0, 10);
        over["arms"] = json!([{"name": "a", "weight": MAX_ARM_WEIGHT + 1}]);
        let err = set_from(vec![(
            "experiment-layer.l",
            layer("organization_id", vec![("a", over)]),
        )])
        .unwrap_err();
        assert_eq!(
            err,
            vec![ExperimentIssue::ArmWeightTooLarge {
                key: "experiment-layer.l: a".into(),
                arm: "a".into(),
                weight: MAX_ARM_WEIGHT + 1
            }]
        );
    }

    #[test]
    fn accepts_arm_weight_at_the_maximum() {
        let mut at = experiment(0, 10);
        at["arms"] = json!([{"name": "a", "weight": MAX_ARM_WEIGHT}]);
        assert!(
            set_from(vec![(
                "experiment-layer.l",
                layer("organization_id", vec![("a", at)]),
            )])
            .is_ok()
        );
    }

    #[test]
    fn accepts_maximum_arms() {
        assert!(
            set_from(vec![(
                "experiment-layer.l",
                layer("organization_id", vec![("a", experiment_with_arms(10))]),
            )])
            .is_ok()
        );
    }

    #[test]
    fn rejects_experiment_name_in_two_layers() {
        let err = set_from(vec![
            (
                "experiment-layer.l1",
                layer("organization_id", vec![("dup", experiment(0, 10))]),
            ),
            (
                "experiment-layer.l2",
                layer("organization_id", vec![("dup", experiment(0, 10))]),
            ),
        ])
        .unwrap_err();
        assert_eq!(
            err,
            vec![ExperimentIssue::DuplicateExperiment {
                experiment: "dup".into(),
                first_layer: "l1".into(),
                second_layer: "l2".into()
            }]
        );
    }

    #[test]
    fn rejects_zero_total_weight_and_duplicate_arms() {
        let mut zero = experiment(0, 10);
        zero["arms"] = json!([{"name": "a", "weight": 0}]);
        let mut dup = experiment(0, 10);
        dup["arms"] = json!([{"name": "a", "weight": 1}, {"name": "a", "weight": 1}]);
        let err = set_from(vec![
            (
                "experiment-layer.dup",
                layer("organization_id", vec![("dup", dup)]),
            ),
            (
                "experiment-layer.zero",
                layer("organization_id", vec![("zero", zero)]),
            ),
        ])
        .unwrap_err();
        assert_eq!(
            err,
            vec![
                ExperimentIssue::DuplicateArm {
                    key: "experiment-layer.dup: dup".into(),
                    arm: "a".into()
                },
                ExperimentIssue::ZeroTotalWeight {
                    key: "experiment-layer.zero: zero".into()
                },
            ]
        );
    }

    #[test]
    fn zero_total_weight_only_fails_when_enabled() {
        let mut disabled = experiment(0, 10);
        disabled["enabled"] = json!(false);
        disabled["arms"] = json!([{"name": "a", "weight": 0}]);
        assert!(
            set_from(vec![(
                "experiment-layer.l",
                layer("organization_id", vec![("disabled", disabled)]),
            )])
            .is_ok()
        );

        let mut enabled = experiment(0, 10);
        enabled["arms"] = json!([{"name": "a", "weight": 0}]);
        let err = set_from(vec![(
            "experiment-layer.m",
            layer("organization_id", vec![("enabled", enabled)]),
        )])
        .unwrap_err();
        assert_eq!(
            err,
            vec![ExperimentIssue::ZeroTotalWeight {
                key: "experiment-layer.m: enabled".into()
            }]
        );
    }

    #[test]
    fn reports_malformed_definition() {
        let err = set_from(vec![(
            "experiment-layer.l",
            json!({"unit": ["organization_id"]}),
        )])
        .unwrap_err();
        assert!(
            matches!(&err[0], ExperimentIssue::InvalidDefinition { key, .. } if key == "experiment-layer.l")
        );
    }

    #[test]
    fn issue_messages_tell_the_owner_what_to_do() {
        let range = ExperimentIssue::AllocationOutOfRange {
            key: "experiment-layer.l: a".into(),
            start: 90,
            size: 20,
        };
        assert_eq!(
            range.to_string(),
            "experiment-layer.l: a: allocation start 90 + size 20 runs past the last slot (99); shrink size or move start"
        );
        let dup = ExperimentIssue::DuplicateExperiment {
            experiment: "a".into(),
            first_layer: "l1".into(),
            second_layer: "l2".into(),
        };
        assert_eq!(
            dup.to_string(),
            "experiment 'a' is declared in both experiment-layer.l1 and experiment-layer.l2; an experiment name belongs to one layer"
        );
    }

    #[test]
    fn allocation_end_saturates_instead_of_overflowing() {
        assert_eq!(
            Allocation {
                start: u32::MAX,
                size: 1
            }
            .end(),
            u32::MAX
        );
    }

    #[test]
    fn total_weight_saturates_instead_of_overflowing() {
        let def = ExperimentDefinition::from_value(&json!({
            "allocation": {"start": 0, "size": 100},
            "arms": [
                {"name": "a", "weight": u64::MAX},
                {"name": "b", "weight": u64::MAX}
            ]
        }))
        .unwrap();
        assert_eq!(def.total_weight(), u64::MAX);
    }

    #[test]
    fn validate_experiments_returns_empty_for_no_experiments() {
        assert!(validate_experiments(&json!({"int-option": 1})).is_empty());
        assert!(validate_experiments(&json!(null)).is_empty());
    }
}
