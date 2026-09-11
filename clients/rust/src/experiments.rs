//! Salted multi-arm experiment assignment with layers.
//!
//! Experiments are `experiment.`-prefixed options (see
//! `sentry_options_validation::experiments`). Two independent hashes decide an
//! assignment: the layer hash picks a slot in `0..LAYER_SLOTS` and the
//! experiment whose allocation covers it owns the subject; the experiment hash
//! then picks the arm by weight. Because membership and arm are separate,
//! shrinking an allocation drops subjects without reshuffling the ones who stay
//! and changing weights never changes membership.
//!
//! Assignment is deterministic and publicly reproducible from the namespace,
//! names and unit values. Never use it to gate anything security-sensitive.

use std::collections::HashMap;

use serde_json::{Value, json};
use sha1::{Digest, Sha1};

pub use sentry_options_validation::experiments::{
    Allocation, Arm, EXPERIMENT_KEY_PREFIX, ExperimentDefinition, ExperimentSet, LAYER_SLOTS,
    LayerMember,
};

/// Fields the unit values are read from, e.g. `{"run_id": 123}`.
pub type ExperimentContext = HashMap<String, Value>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignmentStatus {
    /// The subject is in this experiment's allocation and got an arm.
    Assigned,
    /// The subject's slot belongs to another experiment in the same layer.
    Excluded,
    /// The subject's slot is claimed by no experiment in the layer.
    Holdout,
    /// The subject is in the allocation but the experiment is `enabled: false`.
    Disabled,
    /// Nothing could be decided: not configured, not initialized, or bad context.
    Unassigned,
}

impl AssignmentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Assigned => "assigned",
            Self::Excluded => "excluded",
            Self::Holdout => "holdout",
            Self::Disabled => "disabled",
            Self::Unassigned => "unassigned",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub namespace: String,
    pub experiment: String,
    pub layer: Option<String>,
    pub unit: Vec<String>,
    pub subject: Option<String>,
    pub slot: Option<u32>,
    pub status: AssignmentStatus,
    pub arm: Option<String>,
    pub config: Option<Value>,
    pub excluded_by: Option<String>,
    pub reason: Option<String>,
}

impl Assignment {
    fn unassigned(namespace: &str, experiment: &str, reason: String) -> Self {
        Self {
            namespace: namespace.to_string(),
            experiment: experiment.to_string(),
            layer: None,
            unit: Vec::new(),
            subject: None,
            slot: None,
            status: AssignmentStatus::Unassigned,
            arm: None,
            config: None,
            excluded_by: None,
            reason: Some(reason),
        }
    }

    pub fn is_assigned(&self) -> bool {
        self.status == AssignmentStatus::Assigned
    }

    pub fn in_arm(&self, arm: &str) -> bool {
        self.arm.as_deref() == Some(arm)
    }

    /// The exposure record every service emits; `config` is left out on purpose.
    pub fn to_json(&self) -> Value {
        json!({
            "namespace": self.namespace,
            "experiment": self.experiment,
            "layer": self.layer,
            "unit": self.unit.join(","),
            "subject": self.subject,
            "slot": self.slot,
            "status": self.status.as_str(),
            "arm": self.arm,
            "excluded_by": self.excluded_by,
            "reason": self.reason,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    #[error("Options not initialized - call init() first")]
    NotInitialized,
    #[error(
        "Experiment '{experiment}' expects context field '{field}' to be a string, number or bool"
    )]
    MissingUnit { experiment: String, field: String },
    #[error("Experiments in namespace '{namespace}' are invalid:{message}")]
    InvalidValue { namespace: String, message: String },
    #[error(transparent)]
    Options(#[from] crate::OptionsError),
}

/// SHA-1 over length-prefixed components, first 8 bytes big-endian, modulo.
fn bucket(components: &[&str], modulus: u64) -> u64 {
    let mut hasher = Sha1::new();
    for component in components {
        hasher.update((component.len() as u64).to_be_bytes());
        hasher.update(component.as_bytes());
    }
    let digest = hasher.finalize();
    let raw = u64::from_be_bytes(digest[..8].try_into().expect("SHA-1 digest has 20 bytes"));
    raw % modulus
}

fn unit_value(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Assign `experiment` within an already-loaded set; the checker wraps this with
/// the live values of a namespace.
pub fn assign_in(
    set: &ExperimentSet,
    namespace: &str,
    experiment: &str,
    context: &ExperimentContext,
) -> Result<Assignment, ExperimentError> {
    let Some(def) = set.get(experiment) else {
        return Ok(Assignment::unassigned(
            namespace,
            experiment,
            "experiment is not configured".to_string(),
        ));
    };

    let mut values = Vec::with_capacity(def.unit.len());
    for field in &def.unit {
        let value = context.get(field).and_then(unit_value).ok_or_else(|| {
            ExperimentError::MissingUnit {
                experiment: experiment.to_string(),
                field: field.clone(),
            }
        })?;
        values.push(value);
    }
    let value_refs: Vec<&str> = values.iter().map(String::as_str).collect();

    let mut layer_components = vec!["layer", namespace, def.layer.as_str()];
    layer_components.extend(&value_refs);
    let slot = bucket(&layer_components, LAYER_SLOTS as u64) as u32;

    let mut assignment = Assignment {
        namespace: namespace.to_string(),
        experiment: experiment.to_string(),
        layer: Some(def.layer.clone()),
        unit: def.unit.clone(),
        subject: Some(values.join(":")),
        slot: Some(slot),
        status: AssignmentStatus::Holdout,
        arm: None,
        config: None,
        excluded_by: None,
        reason: None,
    };

    match set.owner_of(&def.layer, slot) {
        None => {}
        Some(owner) if owner.experiment != experiment => {
            assignment.status = AssignmentStatus::Excluded;
            assignment.excluded_by = Some(owner.experiment.clone());
        }
        Some(_) if !def.enabled => assignment.status = AssignmentStatus::Disabled,
        Some(_) => {
            let total = def.total_weight();
            if total == 0 {
                assignment.status = AssignmentStatus::Disabled;
                assignment.reason = Some("arm weights sum to 0".to_string());
                return Ok(assignment);
            }
            let mut arm_components = vec!["experiment", namespace, experiment];
            arm_components.extend(&value_refs);
            let mut point = bucket(&arm_components, total);
            for arm in &def.arms {
                if point < arm.weight {
                    assignment.status = AssignmentStatus::Assigned;
                    assignment.arm = Some(arm.name.clone());
                    assignment.config = arm.config.clone();
                    break;
                }
                point -= arm.weight;
            }
        }
    }
    Ok(assignment)
}

pub struct ExperimentChecker {
    namespace: String,
    options: Option<&'static crate::Options>,
}

impl ExperimentChecker {
    pub fn new(namespace: String, options: &'static crate::Options) -> Self {
        Self {
            namespace,
            options: Some(options),
        }
    }

    /// Never fails: any error becomes an `Unassigned` assignment with a reason.
    pub fn assign(&self, experiment: &str, context: &ExperimentContext) -> Assignment {
        match self.try_assign(experiment, context) {
            Ok(assignment) => assignment,
            Err(e) => {
                tracing::debug!(experiment, error = %e, "Experiment assignment failed");
                Assignment::unassigned(&self.namespace, experiment, e.to_string())
            }
        }
    }

    pub fn try_assign(
        &self,
        experiment: &str,
        context: &ExperimentContext,
    ) -> Result<Assignment, ExperimentError> {
        let opts = self.options.ok_or(ExperimentError::NotInitialized)?;
        let set = opts.experiment_set(&self.namespace)?;
        assign_in(&set, &self.namespace, experiment, context)
    }

    /// Every experiment in `layer`, ordered by allocation start.
    pub fn layer(&self, layer: &str) -> Result<Vec<LayerMember>, ExperimentError> {
        let opts = self.options.ok_or(ExperimentError::NotInitialized)?;
        Ok(opts.experiment_set(&self.namespace)?.layer(layer).to_vec())
    }
}

pub fn experiments(namespace: &str) -> ExperimentChecker {
    ExperimentChecker {
        namespace: namespace.to_string(),
        options: crate::GLOBAL_OPTIONS.get(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap as StdHashMap;

    const NS: &str = "sentry-options-testing";

    fn definition(layer: &str, start: u32, size: u32, arms: &[(&str, u64)]) -> Value {
        json!({
            "owner": {"team": "testing"},
            "layer": layer,
            "unit": ["organization_id"],
            "allocation": {"start": start, "size": size},
            "arms": arms.iter().map(|(name, weight)| json!({"name": name, "weight": weight})).collect::<Vec<_>>()
        })
    }

    fn set(entries: Vec<(&str, Value)>) -> ExperimentSet {
        let map: serde_json::Map<String, Value> = entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        ExperimentSet::from_values(map.iter()).unwrap()
    }

    fn checkout_set() -> ExperimentSet {
        let mut color = definition("checkout", 0, 40, &[("control", 50), ("treatment", 50)]);
        color["arms"][1]["config"] = json!({"color": "green"});
        set(vec![
            ("experiment.checkout-color", color),
            (
                "experiment.checkout-copy",
                definition("checkout", 40, 30, &[("short", 50), ("long", 50)]),
            ),
        ])
    }

    fn ctx(org: i64) -> ExperimentContext {
        StdHashMap::from([("organization_id".to_string(), json!(org))])
    }

    #[test]
    fn bucket_is_length_prefixed_sha1() {
        // Pinned against the Python reference implementation in the docs.
        assert_eq!(bucket(&["layer", NS, "checkout", "1"], 100), 6);
        assert_eq!(bucket(&["layer", NS, "checkout", "16"], 100), 58);
        assert_eq!(bucket(&["layer", NS, "checkout", "4"], 100), 74);
        assert_eq!(bucket(&["layer", NS, "paused", "1"], 100), 13);
    }

    #[test]
    fn pinned_assignments() {
        let set = checkout_set();
        let a = assign_in(&set, NS, "checkout-color", &ctx(1)).unwrap();
        assert_eq!(a.status, AssignmentStatus::Assigned);
        assert_eq!(a.slot, Some(6));
        assert_eq!(a.arm.as_deref(), Some("control"));
        assert_eq!(a.config, None);
        assert_eq!(a.subject.as_deref(), Some("1"));
        assert_eq!(a.layer.as_deref(), Some("checkout"));
        assert_eq!(a.unit, vec!["organization_id"]);

        let a = assign_in(&set, NS, "checkout-color", &ctx(5)).unwrap();
        assert_eq!((a.slot, a.arm.as_deref()), (Some(3), Some("treatment")));
        assert_eq!(a.config, Some(json!({"color": "green"})));

        let a = assign_in(&set, NS, "checkout-color", &ctx(16)).unwrap();
        assert_eq!(a.status, AssignmentStatus::Excluded);
        assert_eq!(a.slot, Some(58));
        assert_eq!(a.excluded_by.as_deref(), Some("checkout-copy"));
        assert_eq!(a.arm, None);

        let a = assign_in(&set, NS, "checkout-copy", &ctx(16)).unwrap();
        assert_eq!(
            (a.status, a.arm.as_deref()),
            (AssignmentStatus::Assigned, Some("short"))
        );
        let a = assign_in(&set, NS, "checkout-copy", &ctx(37)).unwrap();
        assert_eq!((a.slot, a.arm.as_deref()), (Some(45), Some("long")));

        let a = assign_in(&set, NS, "checkout-color", &ctx(4)).unwrap();
        assert_eq!(
            (a.status, a.slot, a.excluded_by),
            (AssignmentStatus::Holdout, Some(74), None)
        );
    }

    #[test]
    fn disabled_experiment_keeps_its_allocation() {
        let mut paused = definition("paused", 0, 100, &[("control", 100)]);
        paused["enabled"] = json!(false);
        paused["unit"] = json!(["run_id"]);
        let set = set(vec![("experiment.paused-experiment", paused)]);
        let context = StdHashMap::from([("run_id".to_string(), json!(1))]);
        let a = assign_in(&set, NS, "paused-experiment", &context).unwrap();
        assert_eq!(
            (a.status, a.slot, a.arm),
            (AssignmentStatus::Disabled, Some(13), None)
        );
    }

    #[test]
    fn unconfigured_experiment_is_unassigned() {
        let a = assign_in(&checkout_set(), NS, "nope", &ctx(1)).unwrap();
        assert_eq!(a.status, AssignmentStatus::Unassigned);
        assert_eq!(a.reason.as_deref(), Some("experiment is not configured"));
        assert!(a.layer.is_none() && a.slot.is_none() && a.subject.is_none());
    }

    #[test]
    fn missing_or_unusable_unit_field_is_an_error() {
        let set = checkout_set();
        let err = assign_in(&set, NS, "checkout-color", &StdHashMap::new()).unwrap_err();
        assert!(
            matches!(err, ExperimentError::MissingUnit { ref field, .. } if field == "organization_id")
        );
        let bad = StdHashMap::from([("organization_id".to_string(), json!([1]))]);
        assert!(matches!(
            assign_in(&set, NS, "checkout-color", &bad),
            Err(ExperimentError::MissingUnit { .. })
        ));
    }

    #[test]
    fn unit_values_are_stringified_like_json() {
        assert_eq!(unit_value(&json!("abc")).as_deref(), Some("abc"));
        assert_eq!(unit_value(&json!(123)).as_deref(), Some("123"));
        assert_eq!(unit_value(&json!(1.5)).as_deref(), Some("1.5"));
        assert_eq!(unit_value(&json!(true)).as_deref(), Some("true"));
        assert_eq!(unit_value(&json!(null)), None);
        assert_eq!(unit_value(&json!({})), None);
    }

    #[test]
    fn composite_unit_hashes_each_value_separately() {
        let mut review = definition("review", 0, 100, &[("terse", 50), ("chatty", 50)]);
        review["unit"] = json!(["organization_id", "pr_id"]);
        let set = set(vec![("experiment.review-tone", review)]);
        let context = StdHashMap::from([
            ("organization_id".to_string(), json!(1)),
            ("pr_id".to_string(), json!(77)),
        ]);
        let a = assign_in(&set, NS, "review-tone", &context).unwrap();
        assert_eq!(a.slot, Some(63));
        assert_eq!(a.subject.as_deref(), Some("1:77"));
        assert_eq!(a.arm.as_deref(), Some("chatty"));
    }

    #[test]
    fn same_layer_is_mutually_exclusive_and_splits_traffic() {
        let set = checkout_set();
        let (mut color, mut copy, mut holdout) = (0, 0, 0);
        for org in 0..20_000 {
            let a = assign_in(&set, NS, "checkout-color", &ctx(org)).unwrap();
            let b = assign_in(&set, NS, "checkout-copy", &ctx(org)).unwrap();
            assert!(
                !(a.is_assigned() && b.is_assigned()),
                "org {org} in both experiments"
            );
            match (a.status, b.status) {
                (AssignmentStatus::Assigned, AssignmentStatus::Excluded) => color += 1,
                (AssignmentStatus::Excluded, AssignmentStatus::Assigned) => copy += 1,
                (AssignmentStatus::Holdout, AssignmentStatus::Holdout) => holdout += 1,
                other => panic!("unexpected pair {other:?}"),
            }
        }
        let frac = |n: i32| n as f64 / 20_000.0;
        assert!((frac(color) - 0.40).abs() < 0.02);
        assert!((frac(copy) - 0.30).abs() < 0.02);
        assert!((frac(holdout) - 0.30).abs() < 0.02);
    }

    #[test]
    fn different_layers_overlap_independently() {
        let set = set(vec![
            (
                "experiment.a",
                definition("layer-a", 0, 100, &[("control", 50), ("treatment", 50)]),
            ),
            (
                "experiment.b",
                definition("layer-b", 0, 100, &[("control", 50), ("treatment", 50)]),
            ),
        ]);
        let mut joint: StdHashMap<(String, String), u32> = StdHashMap::new();
        for org in 0..40_000 {
            let a = assign_in(&set, NS, "a", &ctx(org)).unwrap().arm.unwrap();
            let b = assign_in(&set, NS, "b", &ctx(org)).unwrap().arm.unwrap();
            *joint.entry((a, b)).or_default() += 1;
        }
        for cell in joint.values() {
            assert!((*cell as f64 / 40_000.0 - 0.25).abs() < 0.02);
        }
    }

    #[test]
    fn weights_split_inside_allocation() {
        let set = set(vec![(
            "experiment.w",
            definition("l", 0, 50, &[("big", 90), ("small", 10)]),
        )]);
        let mut big = 0;
        let mut assigned = 0;
        for org in 0..40_000 {
            let a = assign_in(&set, NS, "w", &ctx(org)).unwrap();
            if a.is_assigned() {
                assigned += 1;
                if a.in_arm("big") {
                    big += 1;
                }
            }
        }
        assert!((assigned as f64 / 40_000.0 - 0.5).abs() < 0.02);
        assert!((big as f64 / assigned as f64 - 0.9).abs() < 0.02);
    }

    #[test]
    fn zero_weight_arm_is_never_chosen() {
        let set = set(vec![(
            "experiment.z",
            definition("l", 0, 100, &[("never", 0), ("always", 7)]),
        )]);
        for org in 0..1_000 {
            assert!(
                assign_in(&set, NS, "z", &ctx(org))
                    .unwrap()
                    .in_arm("always")
            );
        }
    }

    #[test]
    fn shrinking_allocation_keeps_remaining_arms() {
        let wide = set(vec![(
            "experiment.s",
            definition("l", 0, 80, &[("control", 50), ("treatment", 50)]),
        )]);
        let narrow = set(vec![(
            "experiment.s",
            definition("l", 0, 30, &[("control", 50), ("treatment", 50)]),
        )]);
        let mut kept = 0;
        for org in 0..5_000 {
            let before = assign_in(&wide, NS, "s", &ctx(org)).unwrap();
            let after = assign_in(&narrow, NS, "s", &ctx(org)).unwrap();
            if after.is_assigned() {
                kept += 1;
                assert!(before.is_assigned());
                assert_eq!(before.arm, after.arm, "org {org} changed arm after shrink");
            }
        }
        assert!(kept > 1_000);
    }

    #[test]
    fn changing_weights_does_not_change_membership() {
        let even = set(vec![(
            "experiment.s",
            definition("l", 10, 50, &[("control", 50), ("treatment", 50)]),
        )]);
        let skewed = set(vec![(
            "experiment.s",
            definition("l", 10, 50, &[("control", 10), ("treatment", 90)]),
        )]);
        for org in 0..2_000 {
            let a = assign_in(&even, NS, "s", &ctx(org)).unwrap();
            let b = assign_in(&skewed, NS, "s", &ctx(org)).unwrap();
            assert_eq!(a.status, b.status);
            assert_eq!(a.slot, b.slot);
        }
    }

    #[test]
    fn colon_in_names_does_not_collide() {
        let set = set(vec![
            (
                "experiment.a",
                definition("x:y", 0, 100, &[("control", 50), ("treatment", 50)]),
            ),
            (
                "experiment.b",
                definition("x", 0, 100, &[("control", 50), ("treatment", 50)]),
            ),
        ]);
        let mut same = 0;
        for org in 0..4_000 {
            let a = assign_in(&set, NS, "a", &ctx(org)).unwrap();
            let b = assign_in(&set, NS, "b", &ctx(org)).unwrap();
            if a.slot == b.slot {
                same += 1;
            }
        }
        assert!(same < 200, "layer slots correlated: {same}");
    }

    #[test]
    fn to_json_is_the_exposure_record_shape() {
        let a = assign_in(&checkout_set(), NS, "checkout-color", &ctx(5)).unwrap();
        assert_eq!(
            a.to_json(),
            json!({
                "namespace": NS,
                "experiment": "checkout-color",
                "layer": "checkout",
                "unit": "organization_id",
                "subject": "5",
                "slot": 3,
                "status": "assigned",
                "arm": "treatment",
                "excluded_by": null,
                "reason": null
            })
        );
    }

    #[test]
    fn checker_without_init_returns_unassigned() {
        let checker = ExperimentChecker {
            namespace: NS.to_string(),
            options: None,
        };
        let a = checker.assign("checkout-color", &ctx(1));
        assert_eq!(a.status, AssignmentStatus::Unassigned);
        assert!(a.reason.unwrap().contains("not initialized"));
        assert!(matches!(
            checker.try_assign("checkout-color", &ctx(1)),
            Err(ExperimentError::NotInitialized)
        ));
    }

    fn write_namespace(dir: &std::path::Path, values: &Value) {
        let schema = json!({
            "version": "1.0",
            "type": "object",
            "properties": {
                "experiment.checkout-color": crate::experiment_property(),
                "experiment.checkout-copy": crate::experiment_property()
            }
        });
        std::fs::create_dir_all(dir.join("schemas/test-ns")).unwrap();
        std::fs::create_dir_all(dir.join("values/test-ns")).unwrap();
        std::fs::write(dir.join("schemas/test-ns/schema.json"), schema.to_string()).unwrap();
        std::fs::write(dir.join("values/test-ns/values.json"), values.to_string()).unwrap();
    }

    fn checkout_values(color_size: u32) -> Value {
        json!({
            "options": {
                "experiment.checkout-color": definition("checkout", 0, color_size, &[("control", 50), ("treatment", 50)]),
                "experiment.checkout-copy": definition("checkout", 40, 30, &[("short", 50), ("long", 50)])
            }
        })
    }

    #[test]
    fn checker_reads_live_values_and_invalidates_cache_on_refresh() {
        let dir = tempfile::TempDir::new().unwrap();
        write_namespace(dir.path(), &checkout_values(40));
        let options: &'static crate::Options = Box::leak(Box::new(
            crate::Options::builder()
                .with_directory(dir.path())
                .with_refresh_threshold(None)
                .build()
                .unwrap(),
        ));
        let checker = ExperimentChecker::new("test-ns".to_string(), options);

        let a = checker.assign("checkout-color", &ctx(1));
        assert_eq!(
            (a.status, a.arm.as_deref()),
            (AssignmentStatus::Assigned, Some("control"))
        );
        let members = checker.layer("checkout").unwrap();
        assert_eq!(
            members
                .iter()
                .map(|m| m.experiment.as_str())
                .collect::<Vec<_>>(),
            vec!["checkout-color", "checkout-copy"]
        );

        write_namespace(dir.path(), &checkout_values(5));
        assert!(options.refresh().unwrap());
        let a = checker.assign("checkout-color", &ctx(1));
        assert_eq!(
            a.status,
            AssignmentStatus::Holdout,
            "slot 6 is outside the shrunk allocation"
        );
    }

    #[test]
    fn checker_reports_unknown_namespace_and_invalid_values() {
        let dir = tempfile::TempDir::new().unwrap();
        write_namespace(dir.path(), &checkout_values(40));
        let options: &'static crate::Options = Box::leak(Box::new(
            crate::Options::builder()
                .with_directory(dir.path())
                .with_refresh_threshold(None)
                .build()
                .unwrap(),
        ));
        let checker = ExperimentChecker::new("nope".to_string(), options);
        assert!(matches!(
            checker.try_assign("checkout-color", &ctx(1)),
            Err(ExperimentError::Options(_))
        ));
        let a = checker.assign("checkout-color", &ctx(1));
        assert_eq!(a.status, AssignmentStatus::Unassigned);
    }

    #[test]
    fn overrides_are_visible_without_refresh() {
        let dir = tempfile::TempDir::new().unwrap();
        write_namespace(dir.path(), &checkout_values(40));
        let options: &'static crate::Options = Box::leak(Box::new(
            crate::Options::builder()
                .with_directory(dir.path())
                .with_refresh_threshold(None)
                .build()
                .unwrap(),
        ));
        let checker = ExperimentChecker::new("test-ns".to_string(), options);
        assert!(checker.assign("checkout-color", &ctx(1)).is_assigned());
        let mut paused = definition("checkout", 0, 40, &[("control", 50), ("treatment", 50)]);
        paused["enabled"] = json!(false);
        crate::testing::set_override("test-ns", "experiment.checkout-color", paused);
        assert_eq!(
            checker.assign("checkout-color", &ctx(1)).status,
            AssignmentStatus::Disabled
        );
        crate::testing::clear_override("test-ns", "experiment.checkout-color");
        assert!(checker.assign("checkout-color", &ctx(1)).is_assigned());
    }
}
