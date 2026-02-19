//! Feature flag evaluation for sentry-options.
//!
//! Feature configurations are stored as JSON strings under `features.{name}` keys.
//! This module provides [`FeatureContext`] and evaluation logic used by [`crate::FeatureChecker`].

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

use serde::Deserialize;
use serde_json::Value as JsonValue;
use sha1::{Digest, Sha1};

// ContextValue

/// A typed value that can be stored in a [`FeatureContext`].
#[derive(Debug, Clone)]
pub enum ContextValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    StringList(Vec<String>),
    IntList(Vec<i64>),
    FloatList(Vec<f64>),
    BoolList(Vec<bool>),
}

impl fmt::Display for ContextValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContextValue::String(s) => write!(f, "{s}"),
            ContextValue::Int(i) => write!(f, "{i}"),
            ContextValue::Float(v) => write!(f, "{v}"),
            // Python str(True) == "True", str(False) == "False"
            ContextValue::Bool(b) => write!(f, "{}", if *b { "True" } else { "False" }),
            ContextValue::StringList(v) => write!(f, "{v:?}"),
            ContextValue::IntList(v) => write!(f, "{v:?}"),
            ContextValue::FloatList(v) => write!(f, "{v:?}"),
            ContextValue::BoolList(v) => write!(f, "{v:?}"),
        }
    }
}

impl From<&str> for ContextValue {
    fn from(s: &str) -> Self {
        ContextValue::String(s.to_string())
    }
}

impl From<String> for ContextValue {
    fn from(s: String) -> Self {
        ContextValue::String(s)
    }
}

impl From<i64> for ContextValue {
    fn from(i: i64) -> Self {
        ContextValue::Int(i)
    }
}

impl From<f64> for ContextValue {
    fn from(f: f64) -> Self {
        ContextValue::Float(f)
    }
}

impl From<bool> for ContextValue {
    fn from(b: bool) -> Self {
        ContextValue::Bool(b)
    }
}

impl From<Vec<String>> for ContextValue {
    fn from(v: Vec<String>) -> Self {
        ContextValue::StringList(v)
    }
}

impl From<Vec<i64>> for ContextValue {
    fn from(v: Vec<i64>) -> Self {
        ContextValue::IntList(v)
    }
}

// FeatureContext

/// Arbitrary application data passed to feature flag evaluation.
///
/// Identity fields determine which context properties are used when computing the
/// stable numeric ID used for rollout decisions.
// Sentinel meaning "not yet computed". Valid values are 0–99.
const ID_NOT_COMPUTED: u8 = 255;

pub struct FeatureContext {
    data: HashMap<String, ContextValue>,
    identity_fields: Vec<String>, // stored sorted lexicographically
    cached_id: AtomicU8,          // SHA1 mod 100, computed lazily; 255 = not computed
}

impl FeatureContext {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            identity_fields: Vec::new(),
            cached_id: AtomicU8::new(ID_NOT_COMPUTED),
        }
    }

    /// Insert a value into the context. Resets the cached id.
    pub fn insert(&mut self, key: impl Into<String>, value: ContextValue) {
        self.data.insert(key.into(), value);
        self.cached_id.store(ID_NOT_COMPUTED, Ordering::Relaxed);
    }

    /// Set the identity fields used for rollout id computation.
    ///
    /// Fields are sorted lexicographically and the cached id is reset.
    pub fn identity_fields(&mut self, fields: Vec<&str>) {
        let mut sorted: Vec<String> = fields.into_iter().map(|s| s.to_string()).collect();
        sorted.sort();
        self.identity_fields = sorted;
        self.cached_id.store(ID_NOT_COMPUTED, Ordering::Relaxed);
    }

    /// Get a value from the context.
    pub fn get(&self, key: &str) -> Option<&ContextValue> {
        self.data.get(key)
    }

    /// Check if a key is present in the context.
    pub fn has(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    fn format_context(&self) -> String {
        let mut pairs: Vec<String> = self.data.iter().map(|(k, v)| format!("{k}={v}")).collect();
        pairs.sort();
        pairs.join(", ")
    }

    /// Return the stable numeric identity for this context (0–99).
    ///
    /// Computed by SHA1-hashing the values of the identity fields (joined with `:`),
    /// then reducing the 20-byte digest to a value in `[0, 100)` via iterative
    /// modular reduction.
    pub fn id(&self) -> u64 {
        let cached = self.cached_id.load(Ordering::Relaxed);
        if cached != ID_NOT_COMPUTED {
            return cached as u64;
        }
        let computed = self.compute_id();
        self.cached_id.store(computed, Ordering::Relaxed);
        computed as u64
    }

    fn compute_id(&self) -> u8 {
        let fields: Vec<&str> = if self.identity_fields.is_empty() {
            let mut keys: Vec<&str> = self.data.keys().map(|k| k.as_str()).collect();
            keys.sort();
            keys
        } else {
            self.identity_fields.iter().map(|s| s.as_str()).collect()
        };

        let mut parts: Vec<String> = Vec::new();
        for field in &fields {
            if let Some(value) = self.data.get(*field) {
                parts.push(value.to_string());
            }
        }

        let id_string = parts.join(":");

        let mut hasher = Sha1::new();
        hasher.update(id_string.as_bytes());
        let hash = hasher.finalize();

        // Iterative mod-100 reduction — matches Python's `int(sha1, 16) % 100`.
        let mut acc: u128 = 0;
        for byte in hash.iter() {
            acc = (acc * 256 + *byte as u128) % 100;
        }
        acc as u8
    }
}

impl Default for FeatureContext {
    fn default() -> Self {
        Self::new()
    }
}

// Serde structs (private)

#[derive(Deserialize)]
pub(crate) struct FeatureData {
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) segments: Vec<SegmentData>,
}

#[derive(Deserialize)]
pub(crate) struct SegmentData {
    pub(crate) name: String,
    #[serde(default = "default_rollout")]
    pub(crate) rollout: u8,
    #[serde(default)]
    pub(crate) conditions: Vec<ConditionData>,
}

fn default_rollout() -> u8 {
    100
}

#[derive(Deserialize)]
pub(crate) struct ConditionData {
    pub(crate) property: String,
    pub(crate) operator: OperatorData,
}

#[derive(Deserialize)]
pub(crate) struct OperatorData {
    pub(crate) kind: OperatorKind,
    pub(crate) value: JsonValue,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum OperatorKind {
    In,
    NotIn,
    Contains,
    NotContains,
    Equals,
    NotEquals,
}

// Debug config

pub(crate) struct DebugConfig {
    pub(crate) log_parse: bool,
    pub(crate) log_match: bool,
    pub(crate) sample_rate: f64,
}

static DEBUG_CONFIG: OnceLock<DebugConfig> = OnceLock::new();

pub(crate) fn debug_config() -> &'static DebugConfig {
    DEBUG_CONFIG.get_or_init(|| {
        let level = std::env::var("SENTRY_OPTIONS_FEATURE_DEBUG_LOG").unwrap_or_default();
        let sample_rate: f64 = std::env::var("SENTRY_OPTIONS_FEATURE_DEBUG_LOG_SAMPLE_RATE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1.0);
        DebugConfig {
            log_parse: level == "all" || level == "parse",
            log_match: level == "all" || level == "match",
            sample_rate,
        }
    })
}

thread_local! {
    static SAMPLE_COUNTER: Cell<u64> = const { Cell::new(0) };
}

impl DebugConfig {
    fn should_sample(&self) -> bool {
        if self.sample_rate >= 1.0 {
            return true;
        }
        if self.sample_rate <= 0.0 {
            return false;
        }
        let interval = (1.0 / self.sample_rate).round() as u64;
        SAMPLE_COUNTER.with(|c| {
            let n = c.get();
            c.set(n.wrapping_add(1));
            n % interval.max(1) == 0
        })
    }
}

// Evaluation

impl FeatureData {
    /// Evaluate this feature config against a context. All errors return `false`.
    pub(crate) fn evaluate(&self, feature_name: &str, ctx: &FeatureContext) -> bool {
        let cfg = debug_config();

        if !self.enabled {
            return false;
        }

        for segment in &self.segments {
            if segment.all_conditions_match(ctx) {
                let result = segment.in_rollout(ctx);
                if cfg.log_match && cfg.should_sample() {
                    eprintln!(
                        "[sentry-options] feature '{}' segment '{}' matched, in_rollout={}",
                        feature_name, segment.name, result
                    );
                }
                if result {
                    return true;
                }
            }
        }

        if cfg.log_match && cfg.should_sample() {
            eprintln!(
                "[sentry-options] feature '{}' did not match: context={{{}}}",
                feature_name,
                ctx.format_context(),
            );
        }
        false
    }
}

impl SegmentData {
    fn all_conditions_match(&self, ctx: &FeatureContext) -> bool {
        for condition in &self.conditions {
            if !condition.evaluate(ctx) {
                return false;
            }
        }
        true
    }

    fn in_rollout(&self, ctx: &FeatureContext) -> bool {
        if self.rollout == 0 {
            return false;
        }
        if self.rollout == 100 {
            return true;
        }
        ctx.id() <= self.rollout as u64
    }
}

impl ConditionData {
    fn evaluate(&self, ctx: &FeatureContext) -> bool {
        let prop = ctx.get(&self.property);
        match &self.operator.kind {
            OperatorKind::In => evaluate_in(prop, &self.operator.value),
            OperatorKind::NotIn => !evaluate_in(prop, &self.operator.value),
            OperatorKind::Contains => evaluate_contains(prop, &self.operator.value),
            OperatorKind::NotContains => !evaluate_contains(prop, &self.operator.value),
            OperatorKind::Equals => evaluate_equals(prop, &self.operator.value),
            OperatorKind::NotEquals => !evaluate_equals(prop, &self.operator.value),
        }
    }
}

/// IN: operator value is an array; property must be a scalar.
fn evaluate_in(prop: Option<&ContextValue>, operator_value: &JsonValue) -> bool {
    let prop = match prop {
        Some(p) => p,
        None => return false,
    };
    // Lists are not valid IN targets
    if matches!(
        prop,
        ContextValue::StringList(_)
            | ContextValue::IntList(_)
            | ContextValue::FloatList(_)
            | ContextValue::BoolList(_)
    ) {
        return false;
    }
    let arr = match operator_value.as_array() {
        Some(a) => a,
        None => return false,
    };
    arr.iter().any(|item| scalar_eq(prop, item))
}

/// CONTAINS: operator value is a scalar; property must be a list.
fn evaluate_contains(prop: Option<&ContextValue>, operator_value: &JsonValue) -> bool {
    let prop = match prop {
        Some(p) => p,
        None => return false,
    };
    match prop {
        ContextValue::StringList(list) => operator_value
            .as_str()
            .map(|s| {
                list.iter()
                    .any(|item| item.to_lowercase() == s.to_lowercase())
            })
            .unwrap_or(false),
        ContextValue::IntList(list) => operator_value
            .as_i64()
            .map(|n| list.contains(&n))
            .unwrap_or(false),
        ContextValue::FloatList(list) => operator_value
            .as_f64()
            .map(|n| list.contains(&n))
            .unwrap_or(false),
        ContextValue::BoolList(list) => operator_value
            .as_bool()
            .map(|b| list.contains(&b))
            .unwrap_or(false),
        _ => false,
    }
}

/// EQUALS: same-type comparison. Strings are case-insensitive.
fn evaluate_equals(prop: Option<&ContextValue>, operator_value: &JsonValue) -> bool {
    let prop = match prop {
        Some(p) => p,
        None => return false,
    };
    scalar_eq(prop, operator_value)
}

/// Case-insensitive string equality, exact equality for other scalar types.
fn scalar_eq(prop: &ContextValue, json_val: &JsonValue) -> bool {
    match prop {
        ContextValue::String(s) => json_val
            .as_str()
            .map(|v| s.to_lowercase() == v.to_lowercase())
            .unwrap_or(false),
        ContextValue::Int(i) => json_val.as_i64().map(|v| *i == v).unwrap_or(false),
        ContextValue::Float(f) => json_val.as_f64().map(|v| *f == v).unwrap_or(false),
        ContextValue::Bool(b) => json_val.as_bool().map(|v| *b == v).unwrap_or(false),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Helper builders to avoid verbose JSON parsing in every test
    fn make_condition(property: &str, kind: OperatorKind, value: JsonValue) -> ConditionData {
        ConditionData {
            property: property.to_string(),
            operator: OperatorData { kind, value },
        }
    }

    fn make_segment(rollout: u8, conditions: Vec<ConditionData>) -> SegmentData {
        SegmentData {
            name: "test-segment".to_string(),
            rollout,
            conditions,
        }
    }

    fn make_feature(enabled: bool, segments: Vec<SegmentData>) -> FeatureData {
        FeatureData { enabled, segments }
    }

    // FeatureContext id tests

    #[test]
    fn test_empty_context_id_matches_python() {
        // SHA1("") reduced mod 100 must equal 5 (matches Python flagpole)
        let ctx = FeatureContext::new();
        assert_eq!(ctx.id() % 100, 5);
    }

    #[test]
    fn test_context_id_with_fields() {
        // SHA1("bar") reduced mod 100 — only values are hashed, not field names
        let mut ctx = FeatureContext::new();
        ctx.identity_fields(vec!["foo"]);
        ctx.insert("foo", "bar".into());
        assert_eq!(ctx.id() % 100, 93);
    }

    #[test]
    fn test_id_is_cached() {
        let mut ctx = FeatureContext::new();
        ctx.identity_fields(vec!["org"]);
        ctx.insert("org", "sentry".into());
        assert_eq!(ctx.id(), ctx.id());
    }

    #[test]
    fn test_id_resets_on_identity_fields_change() {
        let mut ctx = FeatureContext::new();
        ctx.insert("org", "sentry".into());
        ctx.insert("user", "mark".into());
        ctx.identity_fields(vec!["org"]);
        let id1 = ctx.id();
        ctx.identity_fields(vec!["user"]);
        let id2 = ctx.id();
        // Different identity fields → different ids (almost certainly)
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_disabled_feature_returns_false() {
        let feature = make_feature(false, vec![make_segment(100, vec![])]);
        let ctx = FeatureContext::new();
        assert!(!feature.evaluate("test", &ctx));
    }

    #[test]
    fn test_no_segments_returns_false() {
        let feature = make_feature(true, vec![]);
        let ctx = FeatureContext::new();
        assert!(!feature.evaluate("test", &ctx));
    }

    #[test]
    fn test_matching_context_returns_true() {
        let feature = make_feature(
            true,
            vec![make_segment(
                100,
                vec![make_condition(
                    "org",
                    OperatorKind::In,
                    json!(["sentry", "test"]),
                )],
            )],
        );
        let mut ctx = FeatureContext::new();
        ctx.insert("org", "sentry".into());
        assert!(feature.evaluate("test", &ctx));
    }

    #[test]
    fn test_missing_context_field_returns_false() {
        let feature = make_feature(
            true,
            vec![make_segment(
                100,
                vec![make_condition("org", OperatorKind::In, json!(["sentry"]))],
            )],
        );
        let ctx = FeatureContext::new(); // no "org" key
        assert!(!feature.evaluate("test", &ctx));
    }

    #[test]
    fn test_context_key_present_but_value_does_not_match() {
        let feature = make_feature(
            true,
            vec![make_segment(
                100,
                vec![make_condition("org", OperatorKind::In, json!(["sentry"]))],
            )],
        );
        let mut ctx = FeatureContext::new();
        ctx.insert("org", "other".into()); // key present, value not in list
        assert!(!feature.evaluate("test", &ctx));
    }

    #[test]
    fn test_rollout_zero_returns_false() {
        // rollout=0 with no conditions → segment matches but rollout blocks it
        let feature = make_feature(true, vec![make_segment(0, vec![])]);
        let ctx = FeatureContext::new();
        assert!(!feature.evaluate("test", &ctx));
    }

    #[test]
    fn test_rollout_hundred_returns_true() {
        // rollout=100 with no conditions → always passes
        let feature = make_feature(true, vec![make_segment(100, vec![])]);
        let ctx = FeatureContext::new();
        assert!(feature.evaluate("test", &ctx));
    }

    #[test]
    fn test_rollout_determinism() {
        let feature = make_feature(
            true,
            vec![make_segment(
                50,
                vec![make_condition("org", OperatorKind::Equals, json!("sentry"))],
            )],
        );
        let mut ctx = FeatureContext::new();
        ctx.identity_fields(vec!["org"]);
        ctx.insert("org", "sentry".into());
        // Same context → same result every time
        let result = feature.evaluate("test", &ctx);
        for _ in 0..10 {
            assert_eq!(feature.evaluate("test", &ctx), result);
        }
    }

    // Operator tests

    #[test]
    fn test_in_operator_matching() {
        let mut ctx = FeatureContext::new();
        ctx.insert("org", "Sentry".into()); // uppercase — should match case-insensitively

        let cond = make_condition("org", OperatorKind::In, json!(["sentry", "test"]));
        assert!(cond.evaluate(&ctx));
    }

    #[test]
    fn test_in_operator_no_match() {
        let mut ctx = FeatureContext::new();
        ctx.insert("org", "other".into());

        let cond = make_condition("org", OperatorKind::In, json!(["sentry", "test"]));
        assert!(!cond.evaluate(&ctx));
    }

    #[test]
    fn test_not_in_operator() {
        let mut ctx = FeatureContext::new();
        ctx.insert("org", "other".into());

        let cond = make_condition("org", OperatorKind::NotIn, json!(["sentry", "test"]));
        assert!(cond.evaluate(&ctx));

        let cond2 = make_condition("org", OperatorKind::NotIn, json!(["other"]));
        assert!(!cond2.evaluate(&ctx));
    }

    #[test]
    fn test_contains_operator() {
        let mut ctx = FeatureContext::new();
        ctx.insert(
            "orgs",
            ContextValue::StringList(vec!["sentry".to_string(), "test".to_string()]),
        );

        let cond = make_condition("orgs", OperatorKind::Contains, json!("Sentry")); // case-insensitive
        assert!(cond.evaluate(&ctx));

        let cond2 = make_condition("orgs", OperatorKind::Contains, json!("other"));
        assert!(!cond2.evaluate(&ctx));
    }

    #[test]
    fn test_not_contains_operator() {
        let mut ctx = FeatureContext::new();
        ctx.insert("orgs", ContextValue::StringList(vec!["sentry".to_string()]));

        let cond = make_condition("orgs", OperatorKind::NotContains, json!("other"));
        assert!(cond.evaluate(&ctx));

        let cond2 = make_condition("orgs", OperatorKind::NotContains, json!("sentry"));
        assert!(!cond2.evaluate(&ctx));
    }

    #[test]
    fn test_equals_string_case_insensitive() {
        let mut ctx = FeatureContext::new();
        ctx.insert("name", "Sentry".into());

        let cond = make_condition("name", OperatorKind::Equals, json!("sentry"));
        assert!(cond.evaluate(&ctx));
    }

    #[test]
    fn test_equals_int() {
        let mut ctx = FeatureContext::new();
        ctx.insert("count", 42i64.into());

        let cond = make_condition("count", OperatorKind::Equals, json!(42));
        assert!(cond.evaluate(&ctx));

        let cond2 = make_condition("count", OperatorKind::Equals, json!(43));
        assert!(!cond2.evaluate(&ctx));
    }

    #[test]
    fn test_equals_bool() {
        let mut ctx = FeatureContext::new();
        ctx.insert("active", true.into());

        let cond = make_condition("active", OperatorKind::Equals, json!(true));
        assert!(cond.evaluate(&ctx));

        let cond2 = make_condition("active", OperatorKind::Equals, json!(false));
        assert!(!cond2.evaluate(&ctx));
    }

    #[test]
    fn test_equals_type_mismatch_returns_false() {
        // int context vs string condition
        let mut ctx = FeatureContext::new();
        ctx.insert("count", 42i64.into());
        let cond = make_condition("count", OperatorKind::Equals, json!("42"));
        assert!(!cond.evaluate(&ctx));

        // string context vs int condition
        let mut ctx2 = FeatureContext::new();
        ctx2.insert("name", "42".into());
        let cond2 = make_condition("name", OperatorKind::Equals, json!(42));
        assert!(!cond2.evaluate(&ctx2));

        // bool context vs int condition
        let mut ctx3 = FeatureContext::new();
        ctx3.insert("active", true.into());
        let cond3 = make_condition("active", OperatorKind::Equals, json!(1));
        assert!(!cond3.evaluate(&ctx3));
    }

    #[test]
    fn test_not_equals_operator() {
        let mut ctx = FeatureContext::new();
        ctx.insert("status", "active".into());

        let cond = make_condition("status", OperatorKind::NotEquals, json!("inactive"));
        assert!(cond.evaluate(&ctx));

        let cond2 = make_condition("status", OperatorKind::NotEquals, json!("active"));
        assert!(!cond2.evaluate(&ctx));
    }

    // ContextValue Display

    #[test]
    fn test_context_value_display_scalar_types() {
        assert_eq!(
            format!("{}", ContextValue::String("hello".to_string())),
            "hello"
        );
        assert_eq!(format!("{}", ContextValue::Int(42)), "42");
        assert_eq!(format!("{}", ContextValue::Float(3.14)), "3.14");
        assert_eq!(format!("{}", ContextValue::Bool(true)), "True");
        assert_eq!(format!("{}", ContextValue::Bool(false)), "False");
    }

    #[test]
    fn test_context_value_display_list_types() {
        let sl = ContextValue::StringList(vec!["a".to_string(), "b".to_string()]);
        assert!(format!("{sl}").contains("a"));

        let il = ContextValue::IntList(vec![1, 2, 3]);
        assert!(format!("{il}").contains("1"));

        let fl = ContextValue::FloatList(vec![1.5]);
        assert!(format!("{fl}").contains("1.5"));

        let bl = ContextValue::BoolList(vec![true, false]);
        assert!(format!("{bl}").contains("true"));
    }

    // ContextValue From impls

    #[test]
    fn test_context_value_from_string_owned() {
        let cv: ContextValue = String::from("hello").into();
        assert!(matches!(cv, ContextValue::String(s) if s == "hello"));
    }

    #[test]
    fn test_context_value_from_f64() {
        let cv: ContextValue = 3.14f64.into();
        assert!(matches!(cv, ContextValue::Float(f) if (f - 3.14).abs() < 0.001));
    }

    #[test]
    fn test_context_value_from_vec_string() {
        let cv: ContextValue = vec!["a".to_string(), "b".to_string()].into();
        assert!(matches!(cv, ContextValue::StringList(v) if v.len() == 2));
    }

    #[test]
    fn test_context_value_from_vec_i64() {
        let cv: ContextValue = vec![1i64, 2i64, 3i64].into();
        assert!(matches!(cv, ContextValue::IntList(v) if v.len() == 3));
    }

    // FeatureContext::has and Default

    #[test]
    fn test_feature_context_has() {
        let mut ctx = FeatureContext::new();
        assert!(!ctx.has("org"));
        ctx.insert("org", "sentry".into());
        assert!(ctx.has("org"));
        assert!(!ctx.has("user"));
    }

    #[test]
    fn test_feature_context_default_is_empty() {
        let ctx = FeatureContext::default();
        assert!(!ctx.has("anything"));
    }

    // evaluate_in edge cases

    #[test]
    fn test_in_operator_with_list_prop_returns_false() {
        let mut ctx = FeatureContext::new();
        ctx.insert("orgs", ContextValue::StringList(vec!["sentry".to_string()]));
        let cond = make_condition("orgs", OperatorKind::In, json!(["sentry"]));
        assert!(!cond.evaluate(&ctx));
    }

    #[test]
    fn test_in_operator_non_array_value_returns_false() {
        let mut ctx = FeatureContext::new();
        ctx.insert("org", "sentry".into());
        // operator value is a scalar, not an array
        let cond = make_condition("org", OperatorKind::In, json!("sentry"));
        assert!(!cond.evaluate(&ctx));
    }

    // evaluate_contains edge cases

    #[test]
    fn test_contains_missing_prop_returns_false() {
        let ctx = FeatureContext::new(); // no "orgs" key
        let cond = make_condition("orgs", OperatorKind::Contains, json!("sentry"));
        assert!(!cond.evaluate(&ctx));
    }

    #[test]
    fn test_contains_int_list() {
        let mut ctx = FeatureContext::new();
        ctx.insert("ids", ContextValue::IntList(vec![1, 2, 3]));

        let cond = make_condition("ids", OperatorKind::Contains, json!(2));
        assert!(cond.evaluate(&ctx));

        let cond2 = make_condition("ids", OperatorKind::Contains, json!(4));
        assert!(!cond2.evaluate(&ctx));
    }

    #[test]
    fn test_contains_float_list() {
        let mut ctx = FeatureContext::new();
        ctx.insert("rates", ContextValue::FloatList(vec![0.5, 1.0]));

        let cond = make_condition("rates", OperatorKind::Contains, json!(0.5));
        assert!(cond.evaluate(&ctx));

        let cond2 = make_condition("rates", OperatorKind::Contains, json!(0.1));
        assert!(!cond2.evaluate(&ctx));
    }

    #[test]
    fn test_contains_bool_list() {
        let mut ctx = FeatureContext::new();
        ctx.insert("flags", ContextValue::BoolList(vec![true, false]));

        let cond = make_condition("flags", OperatorKind::Contains, json!(true));
        assert!(cond.evaluate(&ctx));

        let cond2 = make_condition("flags", OperatorKind::Contains, json!(false));
        assert!(cond2.evaluate(&ctx));
    }

    #[test]
    fn test_contains_scalar_prop_returns_false() {
        // CONTAINS only works on list types; a scalar context value returns false
        let mut ctx = FeatureContext::new();
        ctx.insert("org", "sentry".into());
        let cond = make_condition("org", OperatorKind::Contains, json!("sentry"));
        assert!(!cond.evaluate(&ctx));
    }

    // scalar_eq edge cases

    #[test]
    fn test_equals_float() {
        let mut ctx = FeatureContext::new();
        ctx.insert("rate", 0.5f64.into());

        let cond = make_condition("rate", OperatorKind::Equals, json!(0.5));
        assert!(cond.evaluate(&ctx));

        let cond2 = make_condition("rate", OperatorKind::Equals, json!(0.1));
        assert!(!cond2.evaluate(&ctx));
    }

    #[test]
    fn test_equals_list_type_returns_false() {
        // Equals on a list context value should return false (no list == scalar)
        let mut ctx = FeatureContext::new();
        ctx.insert("orgs", ContextValue::StringList(vec!["sentry".to_string()]));
        let cond = make_condition("orgs", OperatorKind::Equals, json!("sentry"));
        assert!(!cond.evaluate(&ctx));
    }

    // default_rollout serde default

    #[test]
    fn test_default_rollout_via_deserialization() {
        // When "rollout" is absent, serde should use the default_rollout() fn → 100
        let segment: SegmentData =
            serde_json::from_str(r#"{"name":"test","conditions":[]}"#).unwrap();
        assert_eq!(segment.rollout, 100);
    }

    // DebugConfig::should_sample

    #[test]
    fn test_should_sample_at_rate_one() {
        let cfg = DebugConfig {
            log_parse: false,
            log_match: false,
            sample_rate: 1.0,
        };
        assert!(cfg.should_sample());
    }

    #[test]
    fn test_should_sample_at_rate_zero() {
        let cfg = DebugConfig {
            log_parse: false,
            log_match: false,
            sample_rate: 0.0,
        };
        assert!(!cfg.should_sample());
    }

    #[test]
    fn test_should_sample_at_intermediate_rate() {
        let cfg = DebugConfig {
            log_parse: false,
            log_match: false,
            sample_rate: 0.5,
        };
        // interval = round(1/0.5) = 2: alternates true/false
        let results: Vec<bool> = (0..10).map(|_| cfg.should_sample()).collect();
        assert!(
            results.iter().any(|&r| r),
            "expected at least one sampled call"
        );
        assert!(
            results.iter().any(|&r| !r),
            "expected at least one skipped call"
        );
    }

    #[test]
    fn test_multi_condition_segment_and_logic() {
        // Both conditions must match
        let feature = make_feature(
            true,
            vec![make_segment(
                100,
                vec![
                    make_condition("org", OperatorKind::In, json!(["sentry"])),
                    make_condition("user", OperatorKind::In, json!(["mark@sentry.io"])),
                ],
            )],
        );

        let mut ctx_both = FeatureContext::new();
        ctx_both.insert("org", "sentry".into());
        ctx_both.insert("user", "mark@sentry.io".into());
        assert!(feature.evaluate("test", &ctx_both));

        let mut ctx_one = FeatureContext::new();
        ctx_one.insert("org", "sentry".into());
        ctx_one.insert("user", "other@test.com".into());
        assert!(!feature.evaluate("test", &ctx_one));
    }

    #[test]
    fn test_multi_segment_or_logic() {
        // First matching segment wins
        let feature = make_feature(
            true,
            vec![
                make_segment(
                    100,
                    vec![make_condition("org", OperatorKind::In, json!(["sentry"]))],
                ),
                make_segment(
                    100,
                    vec![make_condition("is_free", OperatorKind::Equals, json!(true))],
                ),
            ],
        );

        let mut ctx1 = FeatureContext::new();
        ctx1.insert("org", "sentry".into());
        assert!(feature.evaluate("test", &ctx1));

        let mut ctx2 = FeatureContext::new();
        ctx2.insert("is_free", true.into());
        assert!(feature.evaluate("test", &ctx2));

        let mut ctx3 = FeatureContext::new();
        ctx3.insert("org", "other".into());
        ctx3.insert("is_free", false.into());
        assert!(!feature.evaluate("test", &ctx3));
    }
}
