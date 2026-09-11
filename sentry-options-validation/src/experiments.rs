//! Typed view of `experiment.`-prefixed values and the cross-key rules JSON
//! Schema can't express: allocations in a layer are disjoint and share a unit.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use serde::Deserialize;
use serde_json::Value;

/// Prefix marking an option key as an experiment.
pub const EXPERIMENT_KEY_PREFIX: &str = "experiment.";
/// Slots per layer; an allocation `size` of 40 is 40% of the layer.
pub const LAYER_SLOTS: u32 = 100;

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
    pub size: u32,
}

impl Allocation {
    /// First slot after the allocation.
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
    pub layer: String,
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
}

/// One experiment's claim on a layer.
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
    ZeroTotalWeight {
        key: String,
    },
    DuplicateArm {
        key: String,
        arm: String,
    },
    Overlap {
        layer: String,
        first: String,
        second: String,
    },
    UnitMismatch {
        layer: String,
        first: String,
        second: String,
    },
}

impl fmt::Display for ExperimentIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = EXPERIMENT_KEY_PREFIX;
        match self {
            Self::InvalidDefinition { key, message } => write!(f, "{key}: {message}"),
            Self::AllocationOutOfRange { key, start, size } => write!(
                f,
                "{key}: allocation start {start} + size {size} runs past the last slot ({}); shrink size or move start",
                LAYER_SLOTS - 1
            ),
            Self::ZeroTotalWeight { key } => write!(
                f,
                "{key}: arm weights sum to 0; give at least one arm a weight, or set enabled: false to pause"
            ),
            Self::DuplicateArm { key, arm } => write!(f, "{key}: arm '{arm}' is declared twice"),
            Self::Overlap {
                layer,
                first,
                second,
            } => write!(
                f,
                "layer '{layer}': allocations of {prefix}{first} and {prefix}{second} overlap; pick free slots, shrink one of them, or move one to another layer"
            ),
            Self::UnitMismatch {
                layer,
                first,
                second,
            } => write!(
                f,
                "layer '{layer}': {prefix}{first} and {prefix}{second} declare different units; every experiment in a layer must use the same unit"
            ),
        }
    }
}

/// Every experiment of one namespace, indexed by name and by layer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExperimentSet {
    experiments: HashMap<String, ExperimentDefinition>,
    layers: HashMap<String, Vec<LayerMember>>,
}

impl ExperimentSet {
    /// Build from a namespace's values; keys without the experiment prefix are ignored.
    pub fn from_values<'a, I>(values: I) -> Result<Self, Vec<ExperimentIssue>>
    where
        I: IntoIterator<Item = (&'a String, &'a Value)>,
    {
        let entries: BTreeMap<&str, &Value> = values
            .into_iter()
            .filter_map(|(key, value)| {
                key.strip_prefix(EXPERIMENT_KEY_PREFIX)
                    .map(|name| (name, value))
            })
            .collect();

        let mut issues = Vec::new();
        let mut experiments = HashMap::new();
        let mut layers: HashMap<String, Vec<LayerMember>> = HashMap::new();
        for (name, value) in entries {
            let key = format!("{EXPERIMENT_KEY_PREFIX}{name}");
            let def = match ExperimentDefinition::from_value(value) {
                Ok(def) => def,
                Err(message) => {
                    issues.push(ExperimentIssue::InvalidDefinition { key, message });
                    continue;
                }
            };
            check_definition(&key, &def, &mut issues);
            layers
                .entry(def.layer.clone())
                .or_default()
                .push(LayerMember {
                    experiment: name.to_string(),
                    allocation: def.allocation,
                    enabled: def.enabled,
                });
            experiments.insert(name.to_string(), def);
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
            for pair in members.windows(2) {
                let (first, second) = (&pair[0], &pair[1]);
                if first.allocation.overlaps(&second.allocation) {
                    issues.push(ExperimentIssue::Overlap {
                        layer: layer.clone(),
                        first: first.experiment.clone(),
                        second: second.experiment.clone(),
                    });
                }
                if experiments[&first.experiment].unit != experiments[&second.experiment].unit {
                    issues.push(ExperimentIssue::UnitMismatch {
                        layer: layer.clone(),
                        first: first.experiment.clone(),
                        second: second.experiment.clone(),
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

    /// Members of `layer` ordered by allocation start; empty for an unknown layer.
    pub fn layer(&self, layer: &str) -> &[LayerMember] {
        self.layers.get(layer).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn owner_of(&self, layer: &str, slot: u32) -> Option<&LayerMember> {
        self.layer(layer)
            .iter()
            .find(|member| member.allocation.contains(slot))
    }

    pub fn is_empty(&self) -> bool {
        self.experiments.is_empty()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        let mut names: Vec<&str> = self.experiments.keys().map(String::as_str).collect();
        names.sort_unstable();
        names.into_iter()
    }
}

fn check_definition(key: &str, def: &ExperimentDefinition, issues: &mut Vec<ExperimentIssue>) {
    if def.allocation.end() > LAYER_SLOTS {
        issues.push(ExperimentIssue::AllocationOutOfRange {
            key: key.to_string(),
            start: def.allocation.start,
            size: def.allocation.size,
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
    }
    if def.total_weight() == 0 {
        issues.push(ExperimentIssue::ZeroTotalWeight {
            key: key.to_string(),
        });
    }
}

/// Cross-key issues in a namespace values object; empty when there are none.
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

    fn experiment(layer: &str, start: u32, size: u32) -> Value {
        json!({
            "owner": {"team": "testing"},
            "layer": layer,
            "unit": ["organization_id"],
            "allocation": {"start": start, "size": size},
            "arms": [{"name": "control", "weight": 50}, {"name": "treatment", "weight": 50}]
        })
    }

    fn set_from(entries: Vec<(&str, Value)>) -> Result<ExperimentSet, Vec<ExperimentIssue>> {
        let map: serde_json::Map<String, Value> = entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ExperimentSet::from_values(map.iter())
    }

    #[test]
    fn parses_definition_with_defaults() {
        let def = ExperimentDefinition::from_value(&experiment("checkout", 0, 40)).unwrap();
        assert!(def.enabled);
        assert_eq!(def.total_weight(), 100);
        assert_eq!(def.allocation, Allocation { start: 0, size: 40 });
        assert_eq!(def.arms[1].config, None);
    }

    #[test]
    fn ignores_keys_without_prefix() {
        let set = set_from(vec![
            ("int-option", json!(3)),
            ("experiment.a", experiment("l", 0, 10)),
        ])
        .unwrap();
        assert_eq!(set.names().collect::<Vec<_>>(), vec!["a"]);
    }

    #[test]
    fn layer_members_sorted_by_start_and_owner_lookup() {
        let set = set_from(vec![
            ("experiment.b", experiment("l", 40, 30)),
            ("experiment.a", experiment("l", 0, 40)),
        ])
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
        let err = set_from(vec![
            ("experiment.a", experiment("l", 0, 50)),
            ("experiment.b", experiment("l", 49, 10)),
        ])
        .unwrap_err();
        assert_eq!(
            err,
            vec![ExperimentIssue::Overlap {
                layer: "l".into(),
                first: "a".into(),
                second: "b".into()
            }]
        );
    }

    #[test]
    fn adjacent_allocations_do_not_overlap() {
        assert!(
            set_from(vec![
                ("experiment.a", experiment("l", 0, 50)),
                ("experiment.b", experiment("l", 50, 50)),
            ])
            .is_ok()
        );
    }

    #[test]
    fn same_allocation_in_different_layers_is_fine() {
        assert!(
            set_from(vec![
                ("experiment.a", experiment("l1", 0, 100)),
                ("experiment.b", experiment("l2", 0, 100)),
            ])
            .is_ok()
        );
    }

    #[test]
    fn rejects_allocation_past_last_slot() {
        let err = set_from(vec![("experiment.a", experiment("l", 90, 20))]).unwrap_err();
        assert_eq!(
            err,
            vec![ExperimentIssue::AllocationOutOfRange {
                key: "experiment.a".into(),
                start: 90,
                size: 20
            }]
        );
    }

    #[test]
    fn rejects_unit_mismatch_within_layer() {
        let mut other = experiment("l", 50, 10);
        other["unit"] = json!(["run_id"]);
        let err = set_from(vec![
            ("experiment.a", experiment("l", 0, 10)),
            ("experiment.b", other),
        ])
        .unwrap_err();
        assert_eq!(
            err,
            vec![ExperimentIssue::UnitMismatch {
                layer: "l".into(),
                first: "a".into(),
                second: "b".into()
            }]
        );
    }

    #[test]
    fn rejects_zero_total_weight_and_duplicate_arms() {
        let mut zero = experiment("l", 0, 10);
        zero["arms"] = json!([{"name": "a", "weight": 0}]);
        let mut dup = experiment("m", 0, 10);
        dup["arms"] = json!([{"name": "a", "weight": 1}, {"name": "a", "weight": 1}]);
        let err = set_from(vec![("experiment.dup", dup), ("experiment.zero", zero)]).unwrap_err();
        assert_eq!(
            err,
            vec![
                ExperimentIssue::DuplicateArm {
                    key: "experiment.dup".into(),
                    arm: "a".into()
                },
                ExperimentIssue::ZeroTotalWeight {
                    key: "experiment.zero".into()
                },
            ]
        );
    }

    #[test]
    fn reports_malformed_definition() {
        let err = set_from(vec![("experiment.a", json!({"layer": "l"}))]).unwrap_err();
        assert!(
            matches!(&err[0], ExperimentIssue::InvalidDefinition { key, .. } if key == "experiment.a")
        );
    }

    #[test]
    fn issue_messages_tell_the_owner_what_to_do() {
        let overlap = ExperimentIssue::Overlap {
            layer: "l".into(),
            first: "a".into(),
            second: "b".into(),
        };
        assert_eq!(
            overlap.to_string(),
            "layer 'l': allocations of experiment.a and experiment.b overlap; pick free slots, shrink one of them, or move one to another layer"
        );
        let range = ExperimentIssue::AllocationOutOfRange {
            key: "experiment.a".into(),
            start: 90,
            size: 20,
        };
        assert_eq!(
            range.to_string(),
            "experiment.a: allocation start 90 + size 20 runs past the last slot (99); shrink size or move start"
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
            "layer": "l",
            "unit": ["organization_id"],
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
