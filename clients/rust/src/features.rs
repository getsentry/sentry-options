//! Feature flag evaluation for sentry-options.
//!
//! Feature configurations are stored as JSON strings under `features.{name}` keys.
//! This module provides [`FeatureContext`] and evaluation logic used by [`crate::FeatureChecker`].

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

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
pub struct FeatureContext {
    data: HashMap<String, ContextValue>,
    identity_fields: Vec<String>, // stored sorted lexicographically
    cached_id: Cell<Option<u8>>,  // SHA1 mod 100, computed lazily
}

impl FeatureContext {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            identity_fields: Vec::new(),
            cached_id: Cell::new(None),
        }
    }

    /// Insert a value into the context. Resets the cached id.
    pub fn insert(&mut self, key: impl Into<String>, value: ContextValue) {
        self.data.insert(key.into(), value);
        self.cached_id.set(None);
    }

    /// Set the identity fields used for rollout id computation.
    ///
    /// Fields are sorted lexicographically and the cached id is reset.
    pub fn identity_fields(&mut self, fields: Vec<&str>) {
        let mut sorted: Vec<String> = fields.into_iter().map(|s| s.to_string()).collect();
        sorted.sort();
        self.identity_fields = sorted;
        self.cached_id.set(None);
    }

    /// Get a value from the context.
    pub fn get(&self, key: &str) -> Option<&ContextValue> {
        self.data.get(key)
    }

    /// Check if a key is present in the context.
    pub fn has(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// Return the stable numeric identity for this context (0–99).
    ///
    /// Computed by SHA1-hashing the values of the identity fields (joined with `:`),
    /// then reducing the 20-byte digest to a value in `[0, 100)` via iterative
    /// modular reduction.
    pub fn id(&self) -> u64 {
        if let Some(cached) = self.cached_id.get() {
            return cached as u64;
        }
        let computed = self.compute_id();
        self.cached_id.set(Some(computed));
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
    #[allow(dead_code)]
    description: Option<String>,
    #[allow(dead_code)]
    owner: Option<OwnerData>,
    #[serde(default)]
    pub(crate) segments: Vec<SegmentData>,
}

#[derive(Deserialize)]
struct OwnerData {
    #[allow(dead_code)]
    team: String,
    #[allow(dead_code)]
    email: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct SegmentData {
    #[allow(dead_code)]
    name: String,
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
    #[allow(dead_code)]
    name: Option<String>,
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
    static SAMPLE_COUNTER: Cell<u64> = Cell::new(0);
}

fn should_sample(sample_rate: f64) -> bool {
    if sample_rate >= 1.0 {
        return true;
    }
    if sample_rate <= 0.0 {
        return false;
    }
    let interval = (1.0 / sample_rate).round() as u64;
    SAMPLE_COUNTER.with(|c| {
        let n = c.get();
        c.set(n.wrapping_add(1));
        n % interval.max(1) == 0
    })
}

// Evaluation

/// Evaluate a parsed feature config against a context. All errors return `false`.
pub(crate) fn evaluate_feature(
    feature_name: &str,
    data: &FeatureData,
    ctx: &FeatureContext,
) -> bool {
    let cfg = debug_config();

    if !data.enabled {
        return false;
    }

    for segment in &data.segments {
        if all_conditions_match(segment, ctx) {
            let result = in_rollout(segment, ctx);
            if cfg.log_match && should_sample(cfg.sample_rate) {
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

    if cfg.log_match && should_sample(cfg.sample_rate) {
        eprintln!(
            "[sentry-options] feature '{}' did not match: context={{{}}}",
            feature_name,
            format_context(ctx),
        );
    }
    false
}

fn format_context(ctx: &FeatureContext) -> String {
    let mut pairs: Vec<String> = ctx.data.iter().map(|(k, v)| format!("{k}={v}")).collect();
    pairs.sort();
    pairs.join(", ")
}

fn all_conditions_match(segment: &SegmentData, ctx: &FeatureContext) -> bool {
    for condition in &segment.conditions {
        if !evaluate_condition(condition, ctx) {
            return false;
        }
    }
    true
}

fn in_rollout(segment: &SegmentData, ctx: &FeatureContext) -> bool {
    if segment.rollout == 0 {
        return false;
    }
    if segment.rollout == 100 {
        return true;
    }
    ctx.id() <= segment.rollout as u64
}

fn evaluate_condition(condition: &ConditionData, ctx: &FeatureContext) -> bool {
    let prop = ctx.get(&condition.property);
    match &condition.operator.kind {
        OperatorKind::In => evaluate_in(prop, &condition.operator.value),
        OperatorKind::NotIn => !evaluate_in(prop, &condition.operator.value),
        OperatorKind::Contains => evaluate_contains(prop, &condition.operator.value),
        OperatorKind::NotContains => !evaluate_contains(prop, &condition.operator.value),
        OperatorKind::Equals => evaluate_equals(prop, &condition.operator.value),
        OperatorKind::NotEquals => !evaluate_equals(prop, &condition.operator.value),
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
            .map(|n| list.iter().any(|&item| item == n))
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
            name: None,
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
        FeatureData {
            enabled,
            description: None,
            owner: None,
            segments,
        }
    }

    // ---- FeatureContext id tests ----

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
        assert!(!evaluate_feature("test", &feature, &ctx));
    }

    #[test]
    fn test_no_segments_returns_false() {
        let feature = make_feature(true, vec![]);
        let ctx = FeatureContext::new();
        assert!(!evaluate_feature("test", &feature, &ctx));
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
        assert!(evaluate_feature("test", &feature, &ctx));
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
        assert!(!evaluate_feature("test", &feature, &ctx));
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
        assert!(!evaluate_feature("test", &feature, &ctx));
    }

    #[test]
    fn test_rollout_zero_returns_false() {
        // rollout=0 with no conditions → segment matches but rollout blocks it
        let feature = make_feature(true, vec![make_segment(0, vec![])]);
        let ctx = FeatureContext::new();
        assert!(!evaluate_feature("test", &feature, &ctx));
    }

    #[test]
    fn test_rollout_hundred_returns_true() {
        // rollout=100 with no conditions → always passes
        let feature = make_feature(true, vec![make_segment(100, vec![])]);
        let ctx = FeatureContext::new();
        assert!(evaluate_feature("test", &feature, &ctx));
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
        let result = evaluate_feature("test", &feature, &ctx);
        for _ in 0..10 {
            assert_eq!(evaluate_feature("test", &feature, &ctx), result);
        }
    }

    // ---- Operator tests ----

    #[test]
    fn test_in_operator_matching() {
        let mut ctx = FeatureContext::new();
        ctx.insert("org", "Sentry".into()); // uppercase — should match case-insensitively

        let cond = make_condition("org", OperatorKind::In, json!(["sentry", "test"]));
        assert!(evaluate_condition(&cond, &ctx));
    }

    #[test]
    fn test_in_operator_no_match() {
        let mut ctx = FeatureContext::new();
        ctx.insert("org", "other".into());

        let cond = make_condition("org", OperatorKind::In, json!(["sentry", "test"]));
        assert!(!evaluate_condition(&cond, &ctx));
    }

    #[test]
    fn test_not_in_operator() {
        let mut ctx = FeatureContext::new();
        ctx.insert("org", "other".into());

        let cond = make_condition("org", OperatorKind::NotIn, json!(["sentry", "test"]));
        assert!(evaluate_condition(&cond, &ctx));

        let cond2 = make_condition("org", OperatorKind::NotIn, json!(["other"]));
        assert!(!evaluate_condition(&cond2, &ctx));
    }

    #[test]
    fn test_contains_operator() {
        let mut ctx = FeatureContext::new();
        ctx.insert(
            "orgs",
            ContextValue::StringList(vec!["sentry".to_string(), "test".to_string()]),
        );

        let cond = make_condition("orgs", OperatorKind::Contains, json!("Sentry")); // case-insensitive
        assert!(evaluate_condition(&cond, &ctx));

        let cond2 = make_condition("orgs", OperatorKind::Contains, json!("other"));
        assert!(!evaluate_condition(&cond2, &ctx));
    }

    #[test]
    fn test_not_contains_operator() {
        let mut ctx = FeatureContext::new();
        ctx.insert("orgs", ContextValue::StringList(vec!["sentry".to_string()]));

        let cond = make_condition("orgs", OperatorKind::NotContains, json!("other"));
        assert!(evaluate_condition(&cond, &ctx));

        let cond2 = make_condition("orgs", OperatorKind::NotContains, json!("sentry"));
        assert!(!evaluate_condition(&cond2, &ctx));
    }

    #[test]
    fn test_equals_string_case_insensitive() {
        let mut ctx = FeatureContext::new();
        ctx.insert("name", "Sentry".into());

        let cond = make_condition("name", OperatorKind::Equals, json!("sentry"));
        assert!(evaluate_condition(&cond, &ctx));
    }

    #[test]
    fn test_equals_int() {
        let mut ctx = FeatureContext::new();
        ctx.insert("count", 42i64.into());

        let cond = make_condition("count", OperatorKind::Equals, json!(42));
        assert!(evaluate_condition(&cond, &ctx));

        let cond2 = make_condition("count", OperatorKind::Equals, json!(43));
        assert!(!evaluate_condition(&cond2, &ctx));
    }

    #[test]
    fn test_equals_bool() {
        let mut ctx = FeatureContext::new();
        ctx.insert("active", true.into());

        let cond = make_condition("active", OperatorKind::Equals, json!(true));
        assert!(evaluate_condition(&cond, &ctx));

        let cond2 = make_condition("active", OperatorKind::Equals, json!(false));
        assert!(!evaluate_condition(&cond2, &ctx));
    }

    #[test]
    fn test_equals_type_mismatch_returns_false() {
        // int context vs string condition
        let mut ctx = FeatureContext::new();
        ctx.insert("count", 42i64.into());
        let cond = make_condition("count", OperatorKind::Equals, json!("42"));
        assert!(!evaluate_condition(&cond, &ctx));

        // string context vs int condition
        let mut ctx2 = FeatureContext::new();
        ctx2.insert("name", "42".into());
        let cond2 = make_condition("name", OperatorKind::Equals, json!(42));
        assert!(!evaluate_condition(&cond2, &ctx2));

        // bool context vs int condition
        let mut ctx3 = FeatureContext::new();
        ctx3.insert("active", true.into());
        let cond3 = make_condition("active", OperatorKind::Equals, json!(1));
        assert!(!evaluate_condition(&cond3, &ctx3));
    }

    #[test]
    fn test_not_equals_operator() {
        let mut ctx = FeatureContext::new();
        ctx.insert("status", "active".into());

        let cond = make_condition("status", OperatorKind::NotEquals, json!("inactive"));
        assert!(evaluate_condition(&cond, &ctx));

        let cond2 = make_condition("status", OperatorKind::NotEquals, json!("active"));
        assert!(!evaluate_condition(&cond2, &ctx));
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
        assert!(evaluate_feature("test", &feature, &ctx_both));

        let mut ctx_one = FeatureContext::new();
        ctx_one.insert("org", "sentry".into());
        ctx_one.insert("user", "other@test.com".into());
        assert!(!evaluate_feature("test", &feature, &ctx_one));
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
        assert!(evaluate_feature("test", &feature, &ctx1));

        let mut ctx2 = FeatureContext::new();
        ctx2.insert("is_free", true.into());
        assert!(evaluate_feature("test", &feature, &ctx2));

        let mut ctx3 = FeatureContext::new();
        ctx3.insert("org", "other".into());
        ctx3.insert("is_free", false.into());
        assert!(!evaluate_feature("test", &feature, &ctx3));
    }
}
