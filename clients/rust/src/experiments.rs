//! Salted multi-arm experiment assignment (v0 spike).
//!
//! A self-contained, opt-in construct for deterministically bucketing a subject
//! into one of several weighted arms. This is intentionally independent of the
//! feature-flag evaluator in [`crate::features`]: it has its own salted hash and
//! never touches [`crate::features::FeatureContext`] or its rollout identity.
//!
//! Assignment is deterministic and publicly reproducible from the salt, subject
//! key, and weights — never use it to gate a security-sensitive decision.

use std::ops::RangeInclusive;

use sha1::{Digest, Sha1};

/// A single weighted arm of an experiment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arm {
    pub name: String,
    pub weight: u32,
}

impl Arm {
    pub fn new(name: impl Into<String>, weight: u32) -> Self {
        Self {
            name: name.into(),
            weight,
        }
    }
}

/// A multi-arm experiment. `namespace` and `name` together salt the assignment,
/// decorrelating experiments that share the same arm layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Experiment {
    pub name: String,
    pub namespace: String,
    pub arms: Vec<Arm>,
}

impl Experiment {
    pub fn new(name: impl Into<String>, namespace: impl Into<String>, arms: Vec<Arm>) -> Self {
        Self {
            name: name.into(),
            namespace: namespace.into(),
            arms,
        }
    }

    /// Assign `subject_key` to one arm; `None` if there are no arms or all
    /// weights are zero. Changing any weight or arm re-buckets every subject.
    pub fn assign(&self, subject_key: &str) -> Option<&Arm> {
        let total: u64 = self.arms.iter().map(|a| a.weight as u64).sum();
        if total == 0 {
            return None;
        }
        let mut bucket = bucket_for([&self.namespace, &self.name, subject_key], total);
        for arm in &self.arms {
            let weight = arm.weight as u64;
            if bucket < weight {
                return Some(arm);
            }
            bucket -= weight;
        }
        unreachable!("bucket {bucket} must be < sum of weights {total}");
    }
}

/// Reduce salted `components` to a bucket in `0..total`. Length-prefixes each
/// component so `:`-bearing values can't collide; SHA-1 is for distribution only.
fn bucket_for(components: [&str; 3], total: u64) -> u64 {
    let mut hasher = Sha1::new();
    for component in components {
        hasher.update((component.len() as u64).to_be_bytes());
        hasher.update(component.as_bytes());
    }
    let digest = hasher.finalize();
    let raw = u64::from_be_bytes(digest[..8].try_into().expect("SHA-1 yields 20 bytes"));
    raw % total
}

/// A layer of mutually exclusive experiments over a single shared salted slot.
///
/// Each experiment claims a disjoint slice of `0..=99`; the caller is
/// responsible for keeping ranges disjoint and within bounds. A subject falling
/// in one experiment's range is excluded from every other experiment in the
/// layer, and any slot no range covers is an implicit holdout.
#[derive(Debug, Clone)]
pub struct Layer {
    pub name: String,
    pub salt: String,
    pub experiments: Vec<(Experiment, RangeInclusive<u32>)>,
}

impl Layer {
    pub fn new(
        name: impl Into<String>,
        salt: impl Into<String>,
        experiments: Vec<(Experiment, RangeInclusive<u32>)>,
    ) -> Self {
        Self {
            name: name.into(),
            salt: salt.into(),
            experiments,
        }
    }

    /// Assign `subject_key` to at most one experiment in the layer, then to an
    /// arm within it. The layer slot (`0..=99`) is computed from the layer's own
    /// salt so it is independent of any single experiment's assignment.
    pub fn assign(&self, subject_key: &str) -> Option<(&Experiment, &Arm)> {
        let slot = bucket_for([&self.salt, &self.name, subject_key], 100) as u32;
        for (experiment, range) in &self.experiments {
            if range.contains(&slot) {
                return experiment.assign(subject_key).map(|arm| (experiment, arm));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn two_arm(name: &str, namespace: &str) -> Experiment {
        Experiment::new(
            name,
            namespace,
            vec![Arm::new("control", 50), Arm::new("treatment", 50)],
        )
    }

    #[test]
    fn determinism_same_subject_same_arm() {
        let exp = two_arm("checkout-color", "seer");
        for i in 0..1000 {
            let key = format!("subject-{i}");
            let first = exp.assign(&key).unwrap().name.clone();
            for _ in 0..10 {
                assert_eq!(exp.assign(&key).unwrap().name, first);
            }
        }
    }

    #[test]
    fn empty_or_zero_weight_returns_none() {
        let empty = Experiment::new("e", "ns", vec![]);
        assert!(empty.assign("s").is_none());
        let zero = Experiment::new("z", "ns", vec![Arm::new("a", 0), Arm::new("b", 0)]);
        assert!(zero.assign("s").is_none());
    }

    #[test]
    fn zero_weight_arm_is_never_selected() {
        let exp = Experiment::new(
            "z",
            "seer",
            vec![Arm::new("never", 0), Arm::new("always", 100)],
        );
        for i in 0..1000 {
            assert_eq!(exp.assign(&format!("s{i}")).unwrap().name, "always");
        }
    }

    #[test]
    fn distribution_50_50() {
        let exp = two_arm("banner-test", "seer");
        let mut counts: HashMap<&str, u32> = HashMap::new();
        let n = 100_000;
        for i in 0..n {
            let key = format!("user:{i}");
            let arm = exp.assign(&key).unwrap();
            *counts.entry(arm.name.as_str()).or_default() += 1;
        }
        let control = counts["control"] as f64 / n as f64;
        let treatment = counts["treatment"] as f64 / n as f64;
        assert!((control - 0.5).abs() < 0.01, "control fraction {control}");
        assert!(
            (treatment - 0.5).abs() < 0.01,
            "treatment fraction {treatment}"
        );
    }

    #[test]
    fn distribution_four_way_even() {
        let exp = Experiment::new(
            "quad",
            "seer",
            vec![
                Arm::new("a", 25),
                Arm::new("b", 25),
                Arm::new("c", 25),
                Arm::new("d", 25),
            ],
        );
        let mut counts: HashMap<String, u32> = HashMap::new();
        let n = 100_000;
        for i in 0..n {
            let key = format!("acct-{i}");
            let arm = exp.assign(&key).unwrap();
            *counts.entry(arm.name.clone()).or_default() += 1;
        }
        for arm in ["a", "b", "c", "d"] {
            let frac = counts[arm] as f64 / n as f64;
            assert!((frac - 0.25).abs() < 0.01, "arm {arm} fraction {frac}");
        }
    }

    #[test]
    fn distribution_uneven_weights() {
        let exp = Experiment::new(
            "weighted",
            "seer",
            vec![Arm::new("big", 90), Arm::new("small", 10)],
        );
        let mut big = 0u32;
        let n = 100_000;
        for i in 0..n {
            if exp.assign(&format!("s{i}")).unwrap().name == "big" {
                big += 1;
            }
        }
        let frac = big as f64 / n as f64;
        assert!((frac - 0.9).abs() < 0.01, "big fraction {frac}");
    }

    #[test]
    fn decorrelation_two_named_experiments() {
        // Two 50/50 experiments differing only in name. If the hash were not
        // salted by name they would correlate perfectly (all 4 joint cells !=
        // 25%). Salting makes the joint distribution ~uniform.
        let exp_a = two_arm("experiment-a", "seer");
        let exp_b = two_arm("experiment-b", "seer");
        let mut joint: HashMap<(String, String), u32> = HashMap::new();
        let n = 100_000;
        for i in 0..n {
            let key = format!("subject#{i}");
            let a = exp_a.assign(&key).unwrap().name.clone();
            let b = exp_b.assign(&key).unwrap().name.clone();
            *joint.entry((a, b)).or_default() += 1;
        }
        for a in ["control", "treatment"] {
            for b in ["control", "treatment"] {
                let cell = joint
                    .get(&(a.to_string(), b.to_string()))
                    .copied()
                    .unwrap_or(0);
                let frac = cell as f64 / n as f64;
                assert!(
                    (frac - 0.25).abs() < 0.01,
                    "joint cell ({a},{b}) fraction {frac} — assignments are correlated"
                );
            }
        }
    }

    #[test]
    fn colon_in_salt_fields_does_not_collide() {
        // Length-prefixed hashing keeps these tuples independent; a naive
        // `:`-join would fold both onto "a:x:b:<key>" and correlate them.
        let a = two_arm("b", "a:x");
        let b = two_arm("x:b", "a");
        let mut joint: HashMap<(String, String), u32> = HashMap::new();
        let n = 100_000;
        for i in 0..n {
            let key = format!("s{i}");
            let x = a.assign(&key).unwrap().name.clone();
            let y = b.assign(&key).unwrap().name.clone();
            *joint.entry((x, y)).or_default() += 1;
        }
        for x in ["control", "treatment"] {
            for y in ["control", "treatment"] {
                let cell = joint
                    .get(&(x.to_string(), y.to_string()))
                    .copied()
                    .unwrap_or(0);
                let frac = cell as f64 / n as f64;
                assert!(
                    (frac - 0.25).abs() < 0.01,
                    "joint cell ({x},{y}) fraction {frac} — colon-bearing fields collided"
                );
            }
        }
    }

    #[test]
    fn layer_mutual_exclusion() {
        let x = two_arm("exp-x", "seer");
        let y = two_arm("exp-y", "seer");
        let layer = Layer::new(
            "onboarding-layer",
            "seer-layer-salt",
            vec![(x, 0..=49), (y, 50..=99)],
        );
        let mut in_x = 0u32;
        let mut in_y = 0u32;
        let mut none = 0u32;
        let n = 100_000;
        for i in 0..n {
            let key = format!("member-{i}");
            match layer.assign(&key) {
                Some((exp, _)) if exp.name == "exp-x" => in_x += 1,
                Some((exp, _)) if exp.name == "exp-y" => in_y += 1,
                Some(_) => unreachable!(),
                None => none += 1,
            }
        }
        // Every subject lands in exactly one experiment (ranges cover 0..=99).
        assert_eq!(none, 0);
        assert_eq!(in_x + in_y, n);
        // 50/50 split of the layer slot between the two experiments.
        let frac_x = in_x as f64 / n as f64;
        assert!((frac_x - 0.5).abs() < 0.01, "layer x fraction {frac_x}");
    }

    #[test]
    fn layer_partial_coverage_leaves_holdout() {
        let x = two_arm("exp-x", "seer");
        let layer = Layer::new("half-layer", "salt", vec![(x, 0..=49)]);
        let mut assigned = 0u32;
        let mut holdout = 0u32;
        let n = 100_000;
        for i in 0..n {
            match layer.assign(&format!("m{i}")) {
                Some(_) => assigned += 1,
                None => holdout += 1,
            }
        }
        let frac = assigned as f64 / n as f64;
        assert!((frac - 0.5).abs() < 0.01, "assigned fraction {frac}");
        assert!(holdout > 0);
    }
}
