use std::collections::HashMap;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

pub use sentry_options_validation::experiments::{
    Allocation, Arm, EXPERIMENT_LAYER_KEY_PREFIX, ExperimentDefinition, ExperimentSet, LAYER_SLOTS,
    LayerMember,
};

pub type ExperimentContext = HashMap<String, Value>;

pub const EXPOSURE_RECORD_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AssignmentStatus {
    Assigned,
    Excluded,
    Holdout,
    Disabled,
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Assignment {
    pub namespace: String,
    pub experiment: String,
    pub layer: Option<String>,
    pub unit: Vec<String>,
    pub subject: Option<String>,
    pub slot: Option<u32>,
    pub allocation_start: Option<u32>,
    pub allocation_size: Option<u32>,
    pub definition_revision: Option<String>,
    pub status: AssignmentStatus,
    pub arm: Option<String>,
    #[serde(skip)]
    pub config: Option<Value>,
    pub excluded_by: Option<String>,
    pub reason: Option<String>,
}

impl Assignment {
    pub fn unassigned(namespace: &str, experiment: &str, reason: String) -> Self {
        Self {
            namespace: namespace.to_string(),
            experiment: experiment.to_string(),
            layer: None,
            unit: Vec::new(),
            subject: None,
            slot: None,
            allocation_start: None,
            allocation_size: None,
            definition_revision: None,
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

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("Assignment serializes")
    }

    pub fn exposure(&self, service: &str) -> Value {
        self.exposure_at(service, Utc::now())
    }

    fn exposure_at(&self, service: &str, recorded_at: DateTime<Utc>) -> Value {
        let mut record = self.to_json();
        let obj = record.as_object_mut().expect("to_json is an object");
        obj.insert("record_version".to_string(), json!(EXPOSURE_RECORD_VERSION));
        obj.insert("service".to_string(), json!(service));
        obj.insert(
            "recorded_at".to_string(),
            json!(recorded_at.to_rfc3339_opts(SecondsFormat::Secs, true)),
        );
        record
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ExperimentError {
    #[error("Options not initialized - call init() first")]
    NotInitialized,
    #[error("Experiment '{experiment}' needs context field '{field}' as a string, number or bool")]
    MissingUnit { experiment: String, field: String },
    #[error("Experiments in namespace '{namespace}' are invalid:{message}")]
    InvalidValue { namespace: String, message: String },
    #[error(transparent)]
    Options(#[from] crate::OptionsError),
}

fn digest_point(components: &[&str]) -> u64 {
    let mut hasher = Sha1::new();
    for component in components {
        hasher.update((component.len() as u64).to_be_bytes());
        hasher.update(component.as_bytes());
    }
    let digest = hasher.finalize();
    u64::from_be_bytes(digest[..8].try_into().expect("SHA-1 digest has 20 bytes"))
}

fn bucket(components: &[&str], modulus: u64) -> u64 {
    digest_point(components) % modulus
}

fn fraction_below(point: u64, cum: u128, total: u64) -> bool {
    (point as u128) * (total as u128) < cum << 64
}

fn select_arm(arms: &[Arm], point: u64, total: u64) -> Option<&Arm> {
    let mut cum: u128 = 0;
    for arm in arms {
        cum += arm.weight as u128;
        if fraction_below(point, cum, total) {
            return Some(arm);
        }
    }
    None
}

fn unit_value(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

pub(crate) fn assign_in(
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
        subject: Some(serde_json::to_string(&values).expect("Vec<String> serializes")),
        slot: Some(slot),
        allocation_start: Some(def.allocation.start),
        allocation_size: Some(def.allocation.size),
        definition_revision: Some(def.revision()),
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
            let point = digest_point(&arm_components);
            if let Some(arm) = select_arm(&def.arms, point, total) {
                assignment.status = AssignmentStatus::Assigned;
                assignment.arm = Some(arm.name.clone());
                assignment.config = arm.config.clone();
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

    pub fn assign(&self, experiment: &str, context: &ExperimentContext) -> Assignment {
        match self.try_assign(experiment, context) {
            Ok(assignment) => assignment,
            Err(e) => {
                let namespace = self.namespace.as_str();
                match &e {
                    ExperimentError::MissingUnit { .. } => {
                        tracing::debug!(namespace, experiment, error = %e, "Experiment assignment failed");
                    }
                    _ => {
                        tracing::warn!(namespace, experiment, error = %e, "Experiment assignment failed");
                    }
                }
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
        let mut layers: serde_json::Map<String, Value> = serde_json::Map::new();
        for (name, mut value) in entries {
            let name = name.to_string();
            let obj = value.as_object_mut().expect("experiment is an object");
            let layer = obj
                .remove("layer")
                .expect("layer field")
                .as_str()
                .expect("layer is a string")
                .to_string();
            let unit = obj.remove("unit").expect("unit field");
            let layer_key = format!("experiment-layer.{layer}");
            if !layers.contains_key(&layer_key) {
                layers.insert(layer_key.clone(), json!({"unit": unit, "experiments": {}}));
            }
            layers
                .get_mut(&layer_key)
                .unwrap()
                .get_mut("experiments")
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert(name, value);
        }
        ExperimentSet::from_values(layers.iter()).unwrap()
    }

    fn checkout_set() -> ExperimentSet {
        let mut color = definition("checkout", 0, 40, &[("control", 50), ("treatment", 50)]);
        color["arms"][1]["config"] = json!({"color": "green"});
        set(vec![
            ("checkout-color", color),
            (
                "checkout-copy",
                definition("checkout", 40, 30, &[("short", 50), ("long", 50)]),
            ),
        ])
    }

    fn ctx(org: i64) -> ExperimentContext {
        StdHashMap::from([("organization_id".to_string(), json!(org))])
    }

    #[test]
    fn bucket_is_length_prefixed_sha1() {
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
        assert_eq!(a.subject.as_deref(), Some(r#"["1"]"#));
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
            (AssignmentStatus::Assigned, Some("long"))
        );
        let a = assign_in(&set, NS, "checkout-copy", &ctx(37)).unwrap();
        assert_eq!((a.slot, a.arm.as_deref()), (Some(45), Some("short")));

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
        let set = set(vec![("paused-experiment", paused)]);
        let context = StdHashMap::from([("run_id".to_string(), json!(1))]);
        let a = assign_in(&set, NS, "paused-experiment", &context).unwrap();
        assert_eq!(
            (a.status, a.slot, a.arm),
            (AssignmentStatus::Disabled, Some(13), None)
        );
    }

    #[test]
    fn excluded_by_disabled_sibling() {
        let mut sibling = definition("split", 0, 50, &[("control", 100)]);
        sibling["enabled"] = json!(false);
        let set = set(vec![
            ("sibling", sibling),
            (
                "other",
                definition("split", 50, 50, &[("a", 50), ("b", 50)]),
            ),
        ]);
        let org = (0..)
            .find(|&org| {
                assign_in(&set, NS, "other", &ctx(org))
                    .unwrap()
                    .slot
                    .unwrap()
                    < 50
            })
            .unwrap();

        let other = assign_in(&set, NS, "other", &ctx(org)).unwrap();
        assert_eq!(other.status, AssignmentStatus::Excluded);
        assert_eq!(other.excluded_by.as_deref(), Some("sibling"));

        let sibling = assign_in(&set, NS, "sibling", &ctx(org)).unwrap();
        assert_eq!(sibling.status, AssignmentStatus::Disabled);
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
        let set = set(vec![("review-tone", review)]);
        let context = StdHashMap::from([
            ("organization_id".to_string(), json!(1)),
            ("pr_id".to_string(), json!(77)),
        ]);
        let a = assign_in(&set, NS, "review-tone", &context).unwrap();
        assert_eq!(a.slot, Some(63));
        assert_eq!(a.subject.as_deref(), Some(r#"["1","77"]"#));
        assert_eq!(a.arm.as_deref(), Some("terse"));
    }

    #[test]
    fn subject_encoding_does_not_collide_on_colon() {
        let mut exp = definition("l", 0, 100, &[("a", 50), ("b", 50)]);
        exp["unit"] = json!(["u1", "u2"]);
        let set = set(vec![("e", exp)]);
        let left = StdHashMap::from([
            ("u1".to_string(), json!("a:b")),
            ("u2".to_string(), json!("c")),
        ]);
        let right = StdHashMap::from([
            ("u1".to_string(), json!("a")),
            ("u2".to_string(), json!("b:c")),
        ]);
        let ls = assign_in(&set, NS, "e", &left).unwrap().subject;
        let rs = assign_in(&set, NS, "e", &right).unwrap().subject;
        assert_ne!(ls, rs);
        assert_eq!(ls.as_deref(), Some(r#"["a:b","c"]"#));
        assert_eq!(rs.as_deref(), Some(r#"["a","b:c"]"#));
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
                "a",
                definition("layer-a", 0, 100, &[("control", 50), ("treatment", 50)]),
            ),
            (
                "b",
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
            "w",
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
            "z",
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
            "s",
            definition("l", 0, 80, &[("control", 50), ("treatment", 50)]),
        )]);
        let narrow = set(vec![(
            "s",
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
    fn new_experiment_name_reshuffles_arms_without_changing_membership() {
        let experiment_definition =
            definition("models", 10, 40, &[("control", 1), ("treatment", 1)]);
        let original = set(vec![("model-test-v1", experiment_definition.clone())]);
        let restarted = set(vec![("model-test-v3", experiment_definition)]);
        let mut assigned_count = 0;
        let mut switched_count = 0;

        for organization_id in 0..5_000 {
            let context = ctx(organization_id);
            let before = assign_in(&original, NS, "model-test-v1", &context).unwrap();
            let after = assign_in(&restarted, NS, "model-test-v3", &context).unwrap();

            assert_eq!(before.status, after.status);
            assert_eq!(before.slot, after.slot);
            assert_eq!(before.subject, after.subject);
            assert_eq!(before.definition_revision, after.definition_revision);
            assert_ne!(before.experiment, after.experiment);

            if before.is_assigned() {
                assigned_count += 1;
                if before.arm != after.arm {
                    switched_count += 1;
                }
            }
        }

        assert!(assigned_count > 1_000);
        assert!(switched_count * 100 > assigned_count * 40);
        assert!(switched_count * 100 < assigned_count * 60);
    }

    #[test]
    fn changing_weights_does_not_change_membership() {
        let even = set(vec![(
            "s",
            definition("l", 10, 50, &[("control", 50), ("treatment", 50)]),
        )]);
        let skewed = set(vec![(
            "s",
            definition("l", 10, 50, &[("control", 10), ("treatment", 90)]),
        )]);
        for org in 0..2_000 {
            let a = assign_in(&even, NS, "s", &ctx(org)).unwrap();
            let b = assign_in(&skewed, NS, "s", &ctx(org)).unwrap();
            assert_eq!(a.status, b.status);
            assert_eq!(a.slot, b.slot);
        }
    }

    fn scale_set(weights: &[(&str, u64)]) -> ExperimentSet {
        set(vec![("s", definition("scale", 0, 100, weights))])
    }

    #[test]
    fn arm_selection_is_invariant_under_proportional_weight_scaling() {
        let even = scale_set(&[("control", 50), ("treatment", 50)]);
        for scaled in [
            scale_set(&[("control", 1), ("treatment", 1)]),
            scale_set(&[("control", 100), ("treatment", 100)]),
            scale_set(&[("control", 25), ("treatment", 25)]),
        ] {
            for org in 0..2_000 {
                assert_eq!(
                    assign_in(&even, NS, "s", &ctx(org)).unwrap().arm,
                    assign_in(&scaled, NS, "s", &ctx(org)).unwrap().arm,
                    "org {org} changed arm under scaling"
                );
            }
        }

        let tilted = scale_set(&[("a", 30), ("b", 70)]);
        for scaled in [
            scale_set(&[("a", 3), ("b", 7)]),
            scale_set(&[("a", 300), ("b", 700)]),
        ] {
            for org in 0..2_000 {
                assert_eq!(
                    assign_in(&tilted, NS, "s", &ctx(org)).unwrap().arm,
                    assign_in(&scaled, NS, "s", &ctx(org)).unwrap().arm,
                );
            }
        }
    }

    #[test]
    fn three_arm_split_is_invariant_under_scaling() {
        let base = scale_set(&[("a", 20), ("b", 30), ("c", 50)]);
        let scaled = scale_set(&[("a", 2), ("b", 3), ("c", 5)]);
        for org in 0..2_000 {
            assert_eq!(
                assign_in(&base, NS, "s", &ctx(org)).unwrap().arm,
                assign_in(&scaled, NS, "s", &ctx(org)).unwrap().arm,
            );
        }
    }

    #[test]
    fn colon_in_names_does_not_collide() {
        let set = set(vec![
            (
                "a",
                definition("x:y", 0, 100, &[("control", 50), ("treatment", 50)]),
            ),
            (
                "b",
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
                "unit": ["organization_id"],
                "subject": "[\"5\"]",
                "slot": 3,
                "allocation_start": 0,
                "allocation_size": 40,
                "definition_revision": "b6df71182b5566f6",
                "status": "assigned",
                "arm": "treatment",
                "excluded_by": null,
                "reason": null
            })
        );
    }

    #[test]
    fn allocation_is_recorded_for_every_configured_status() {
        let checkout = checkout_set();

        let assigned = assign_in(&checkout, NS, "checkout-color", &ctx(1)).unwrap();
        assert_eq!(assigned.status, AssignmentStatus::Assigned);
        assert_eq!(
            (assigned.allocation_start, assigned.allocation_size),
            (Some(0), Some(40))
        );

        let excluded = assign_in(&checkout, NS, "checkout-color", &ctx(16)).unwrap();
        assert_eq!(excluded.status, AssignmentStatus::Excluded);
        assert_eq!(
            (excluded.allocation_start, excluded.allocation_size),
            (Some(0), Some(40))
        );

        let holdout = assign_in(&checkout, NS, "checkout-color", &ctx(4)).unwrap();
        assert_eq!(holdout.status, AssignmentStatus::Holdout);
        assert_eq!(
            (holdout.allocation_start, holdout.allocation_size),
            (Some(0), Some(40))
        );

        let mut paused = definition("paused", 0, 100, &[("control", 100)]);
        paused["enabled"] = json!(false);
        paused["unit"] = json!(["run_id"]);
        let paused_set = set(vec![("paused-experiment", paused)]);
        let context = StdHashMap::from([("run_id".to_string(), json!(1))]);
        let disabled = assign_in(&paused_set, NS, "paused-experiment", &context).unwrap();
        assert_eq!(disabled.status, AssignmentStatus::Disabled);
        assert_eq!(
            (disabled.allocation_start, disabled.allocation_size),
            (Some(0), Some(100))
        );

        let unconfigured = assign_in(&checkout, NS, "nope", &ctx(1)).unwrap();
        assert_eq!(unconfigured.status, AssignmentStatus::Unassigned);
        assert_eq!(
            (unconfigured.allocation_start, unconfigured.allocation_size),
            (None, None)
        );
    }

    #[test]
    fn to_json_carries_allocation_numbers_and_null_when_unassigned() {
        let assigned = assign_in(&checkout_set(), NS, "checkout-color", &ctx(1))
            .unwrap()
            .to_json();
        assert_eq!(assigned["allocation_start"], json!(0));
        assert_eq!(assigned["allocation_size"], json!(40));

        let unassigned =
            Assignment::unassigned(NS, "checkout-color", "not configured".to_string()).to_json();
        assert_eq!(unassigned["allocation_start"], Value::Null);
        assert_eq!(unassigned["allocation_size"], Value::Null);
    }

    #[test]
    fn exposure_appends_version_service_and_recorded_at() {
        let a = assign_in(&checkout_set(), NS, "checkout-color", &ctx(1)).unwrap();
        let recorded_at: DateTime<Utc> = "2026-09-14T17:03:09Z".parse().unwrap();
        assert_eq!(
            a.exposure_at("seer", recorded_at),
            json!({
                "namespace": NS,
                "experiment": "checkout-color",
                "layer": "checkout",
                "unit": ["organization_id"],
                "subject": "[\"1\"]",
                "slot": 6,
                "allocation_start": 0,
                "allocation_size": 40,
                "definition_revision": "b6df71182b5566f6",
                "status": "assigned",
                "arm": "control",
                "excluded_by": null,
                "reason": null,
                "record_version": 1,
                "service": "seer",
                "recorded_at": "2026-09-14T17:03:09Z",
            })
        );
        assert_eq!(EXPOSURE_RECORD_VERSION, 1);
    }

    #[test]
    fn exposure_uses_now_for_recorded_at() {
        let a = assign_in(&checkout_set(), NS, "checkout-color", &ctx(1)).unwrap();
        let before = Utc::now();
        let record = a.exposure("seer");
        let recorded_at: DateTime<Utc> = record["recorded_at"].as_str().unwrap().parse().unwrap();
        assert!((recorded_at - before).num_seconds().abs() <= 5);
        assert_eq!(record["service"], json!("seer"));
        assert_eq!(record["record_version"], json!(1));
    }

    #[test]
    fn assignment_carries_definition_revision() {
        let set = checkout_set();
        let a = assign_in(&set, NS, "checkout-color", &ctx(1)).unwrap();
        let rev = a
            .definition_revision
            .clone()
            .expect("configured status has a revision");
        assert_eq!(rev.len(), 16);
        assert_eq!(a.to_json()["definition_revision"], json!(rev));
        let recorded_at: DateTime<Utc> = "2026-09-14T17:03:09Z".parse().unwrap();
        assert_eq!(
            a.exposure_at("seer", recorded_at)["definition_revision"],
            json!(rev)
        );

        let unassigned = Assignment::unassigned(NS, "checkout-color", "x".to_string());
        assert_eq!(unassigned.definition_revision, None);
        assert_eq!(unassigned.to_json()["definition_revision"], Value::Null);
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
                "experiment-layer.checkout": crate::experiment_layer_property()
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
                "experiment-layer.checkout": {
                    "unit": ["organization_id"],
                    "experiments": {
                        "checkout-color": {
                            "owner": {"team": "testing"},
                            "allocation": {"start": 0, "size": color_size},
                            "arms": [{"name": "control", "weight": 50}, {"name": "treatment", "weight": 50}]
                        },
                        "checkout-copy": {
                            "owner": {"team": "testing"},
                            "allocation": {"start": 40, "size": 30},
                            "arms": [{"name": "short", "weight": 50}, {"name": "long", "weight": 50}]
                        }
                    }
                }
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

        let a = checker.assign("checkout-color", &ctx(13));
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

        write_namespace(dir.path(), &checkout_values(10));
        assert!(options.refresh().unwrap());
        let a = checker.assign("checkout-color", &ctx(13));
        assert_eq!(
            a.status,
            AssignmentStatus::Holdout,
            "slot 21 is outside the shrunk allocation"
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
        let paused_layer = json!({
            "unit": ["organization_id"],
            "experiments": {
                "checkout-color": {
                    "owner": {"team": "testing"},
                    "allocation": {"start": 0, "size": 40},
                    "enabled": false,
                    "arms": [{"name": "control", "weight": 50}, {"name": "treatment", "weight": 50}]
                }
            }
        });
        crate::testing::set_override("test-ns", "experiment-layer.checkout", paused_layer);
        assert_eq!(
            checker.assign("checkout-color", &ctx(1)).status,
            AssignmentStatus::Disabled
        );
        crate::testing::clear_override("test-ns", "experiment-layer.checkout");
        assert!(checker.assign("checkout-color", &ctx(1)).is_assigned());
    }
}
