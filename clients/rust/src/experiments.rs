//! Salted multi-arm experiment assignment (v0 spike).
//!
//! A self-contained, opt-in construct for deterministically bucketing a subject
//! into one of several weighted arms. This is intentionally independent of the
//! feature-flag evaluator in [`crate::features`]: it has its own salted hash and
//! never touches [`crate::features::FeatureContext`] or its rollout identity.

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

/// A multi-arm experiment. `namespace` acts as the salt that decorrelates
/// distinct experiments sharing the same arm layout.
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
}

/// Reduce a salted string to a bucket in `0..total` using the first 8 bytes of
/// its SHA-1 digest. Own hash, kept separate from the feature evaluator's.
fn bucket_of(salted: &str, total: u32) -> u32 {
    let mut hasher = Sha1::new();
    hasher.update(salted.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&digest[0..8]);
    let raw = u64::from_be_bytes(bytes);
    (raw % total as u64) as u32
}

fn salted_key(namespace: &str, name: &str, subject_key: &str) -> String {
    format!("{namespace}:{name}:{subject_key}")
}

/// Deterministically assign `subject_key` to one of `experiment`'s arms.
///
/// Returns `None` only when the experiment has no arms or all weights are zero.
/// The salt (namespace + name) means two experiments with identical arm layouts
/// but different names assign independently.
pub fn assign<'a>(experiment: &'a Experiment, subject_key: &str) -> Option<&'a Arm> {
    let total: u32 = experiment.arms.iter().map(|a| a.weight).sum();
    if total == 0 {
        return None;
    }
    let salted = salted_key(&experiment.namespace, &experiment.name, subject_key);
    let mut bucket = bucket_of(&salted, total);
    for arm in &experiment.arms {
        if bucket < arm.weight {
            return Some(arm);
        }
        bucket -= arm.weight;
    }
    // Unreachable: bucket < total == sum(weights). Kept as a total fallback.
    experiment.arms.last()
}

/// A layer of mutually exclusive experiments over a single shared salted bucket.
///
/// Each experiment claims a disjoint slice of `0..=99`. A subject falling in one
/// experiment's range is excluded from every other experiment in the layer.
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
        let salted = salted_key(&self.salt, &self.name, subject_key);
        let slot = bucket_of(&salted, 100);
        for (experiment, range) in &self.experiments {
            if range.contains(&slot) {
                return assign(experiment, subject_key).map(|arm| (experiment, arm));
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
            let first = assign(&exp, &key).unwrap().name.clone();
            for _ in 0..10 {
                assert_eq!(assign(&exp, &key).unwrap().name, first);
            }
        }
    }

    #[test]
    fn empty_or_zero_weight_returns_none() {
        let empty = Experiment::new("e", "ns", vec![]);
        assert!(assign(&empty, "s").is_none());
        let zero = Experiment::new("z", "ns", vec![Arm::new("a", 0), Arm::new("b", 0)]);
        assert!(assign(&zero, "s").is_none());
    }

    #[test]
    fn distribution_50_50() {
        let exp = two_arm("banner-test", "seer");
        let mut counts: HashMap<&str, u32> = HashMap::new();
        let n = 100_000;
        for i in 0..n {
            let key = format!("user:{i}");
            let arm = assign(&exp, &key).unwrap();
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
            let arm = assign(&exp, &key).unwrap();
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
            if assign(&exp, &format!("s{i}")).unwrap().name == "big" {
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
            let a = assign(&exp_a, &key).unwrap().name.clone();
            let b = assign(&exp_b, &key).unwrap().name.clone();
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
