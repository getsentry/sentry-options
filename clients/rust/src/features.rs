//! Feature flag evaluation.
//!
//! Provides [`FeatureContext`], [`ContextValue`], and [`FeatureChecker`] for
//! evaluating feature flags stored in the options system.

use num::bigint::{BigInt, Sign};
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;
use sha1::{Digest, Sha1};

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

impl From<i32> for ContextValue {
    fn from(i: i32) -> Self {
        ContextValue::Int(i as i64)
    }
}

impl From<i64> for ContextValue {
    fn from(i: i64) -> Self {
        ContextValue::Int(i)
    }
}

impl From<f32> for ContextValue {
    fn from(f: f32) -> Self {
        ContextValue::Float(f as f64)
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

impl From<Vec<f64>> for ContextValue {
    fn from(v: Vec<f64>) -> Self {
        ContextValue::FloatList(v)
    }
}

impl From<Vec<bool>> for ContextValue {
    fn from(v: Vec<bool>) -> Self {
        ContextValue::BoolList(v)
    }
}

impl ContextValue {
    /// Produce a Python-compatible string representation for identity hashing.
    fn to_id_string(&self) -> String {
        match self {
            ContextValue::String(s) => s.clone(),
            ContextValue::Int(i) => i.to_string(),
            ContextValue::Float(f) => {
                // Match Python's str() output: 1.0 -> "1.0", 1.5 -> "1.5"
                if f.fract() == 0.0 {
                    format!("{f:.1}")
                } else {
                    f.to_string()
                }
            }
            ContextValue::Bool(b) => if *b { "True" } else { "False" }.to_string(),
            ContextValue::StringList(v) => format!("{v:?}"),
            ContextValue::IntList(v) => format!("{v:?}"),
            ContextValue::FloatList(v) => format!("{v:?}"),
            ContextValue::BoolList(v) => format!("{v:?}"),
        }
    }
}

/// Application context passed to feature flag evaluation.
///
/// Contains arbitrary key-value data used to evaluate feature flag conditions.
/// The identity fields determine which fields are used for rollout bucketing.
pub struct FeatureContext {
    data: HashMap<String, ContextValue>,
    identity_fields: Vec<String>,
    cached_id: Cell<Option<u64>>,
}

impl FeatureContext {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            identity_fields: Vec::new(),
            cached_id: Cell::new(None),
        }
    }

    /// Set the fields used to compute this context's rollout identity.
    ///
    /// Fields are sorted lexicographically before hashing. Calling this
    /// resets the cached identity value.
    pub fn identity_fields(&mut self, fields: Vec<&str>) {
        self.identity_fields = fields.into_iter().map(|s| s.to_string()).collect();
        self.cached_id.set(None);
    }

    /// Insert a key-value pair into the context.
    pub fn insert(&mut self, key: &str, value: ContextValue) {
        self.data.insert(key.to_string(), value);
        self.cached_id.set(None);
    }

    /// Get a context value by key.
    pub fn get(&self, key: &str) -> Option<&ContextValue> {
        self.data.get(key)
    }

    /// Check if a key is present in the context.
    pub fn has(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// Compute and return this context's rollout identity.
    ///
    /// The result is cached and reset when identity fields or context data change.
    /// When no identity fields are set, all context keys are used (non-deterministic
    /// rollout across contexts with different keys).
    /// The id value is mostly used to id % 100 < rollout.
    pub fn id(&self) -> u64 {
        if let Some(id) = self.cached_id.get() {
            return id;
        }
        let id = self.compute_id();
        self.cached_id.set(Some(id));
        id
    }

    /// Compute the id for a FeatureContext.
    ///
    /// The original python implementation used a bigint value
    /// derived from the sha1 hash.
    ///
    /// This method returns a u64 which contains the lower place
    /// values of the bigint so that our rollout modulo math is
    /// consistent with the original python implementation.
    fn compute_id(&self) -> u64 {
        let mut identity_fields: Vec<&String> = self
            .identity_fields
            .iter()
            .filter(|f| self.data.contains_key(f.as_str()))
            .collect();
        if identity_fields.is_empty() {
            identity_fields = self.data.keys().collect();
        }
        identity_fields.sort();

        let mut parts: Vec<String> = Vec::with_capacity(identity_fields.len() * 2);
        for key in identity_fields {
            parts.push(key.clone());
            parts.push(self.data[key.as_str()].to_id_string());
        }
        let mut hasher = Sha1::new();
        hasher.update(parts.join(":").as_bytes());
        let digest = hasher.finalize();

        // Create a BigInt to preserve all the 20bytes of the hash digest.
        let bigint = BigInt::from_bytes_be(Sign::Plus, digest.as_slice());

        // We only need the lower places from the big int to retain compatiblity
        // modulo will trim off the u64 overflow, and let us break the bigint
        // into its pieces (there will only be one).
        let small: BigInt = bigint % 1000000000;

        small.to_u64_digits().1[0]
    }
}

impl Default for FeatureContext {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
enum OperatorKind {
    In,
    NotIn,
    Contains,
    NotContains,
    Equals,
    NotEquals,
}

#[derive(Debug)]
struct Condition {
    property: String,
    operator: OperatorKind,
    value: Value,
}

#[derive(Debug)]
struct Segment {
    rollout: u64,
    conditions: Vec<Condition>,
}

#[derive(Debug)]
struct Feature {
    enabled: bool,
    segments: Vec<Segment>,
}

impl Feature {
    fn from_json(value: &Value) -> Option<Self> {
        // Default to false per spec: "If undefined, enabled defaults to false"
        let enabled = value
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let segments = value
            .get("segments")?
            .as_array()?
            .iter()
            .filter_map(Segment::from_json)
            .collect();
        Some(Feature { enabled, segments })
    }

    fn matches(&self, context: &FeatureContext) -> bool {
        if !self.enabled {
            return false;
        }
        for segment in &self.segments {
            if segment.conditions_match(context) {
                return segment.in_rollout(context);
            }
        }
        false
    }
}

impl Segment {
    fn from_json(value: &Value) -> Option<Self> {
        let rollout = value.get("rollout").and_then(|v| v.as_u64()).unwrap_or(0);
        let conditions = value
            .get("conditions")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(Condition::from_json).collect())
            .unwrap_or_default();
        Some(Segment {
            rollout,
            conditions,
        })
    }

    fn conditions_match(&self, context: &FeatureContext) -> bool {
        self.conditions.iter().all(|c| c.matches(context))
    }

    fn in_rollout(&self, context: &FeatureContext) -> bool {
        if self.rollout == 0 {
            return false;
        }
        if self.rollout >= 100 {
            return true;
        }
        context.id() % 100 < self.rollout
    }
}

impl Condition {
    fn from_json(value: &Value) -> Option<Self> {
        let property = value.get("property")?.as_str()?.to_string();
        let operator = match value.get("operator")?.as_str()? {
            "in" => OperatorKind::In,
            "not_in" => OperatorKind::NotIn,
            "contains" => OperatorKind::Contains,
            "not_contains" => OperatorKind::NotContains,
            "equals" => OperatorKind::Equals,
            "not_equals" => OperatorKind::NotEquals,
            _ => return None,
        };
        let value = value.get("value")?.clone();
        Some(Condition {
            property,
            operator,
            value,
        })
    }

    fn matches(&self, context: &FeatureContext) -> bool {
        let Some(ctx_val) = context.get(&self.property) else {
            return false;
        };
        match &self.operator {
            OperatorKind::In => eval_in(ctx_val, &self.value),
            OperatorKind::NotIn => !eval_in(ctx_val, &self.value),
            OperatorKind::Contains => eval_contains(ctx_val, &self.value),
            OperatorKind::NotContains => !eval_contains(ctx_val, &self.value),
            OperatorKind::Equals => eval_equals(ctx_val, &self.value),
            OperatorKind::NotEquals => !eval_equals(ctx_val, &self.value),
        }
    }
}

fn eval_in(ctx_val: &ContextValue, condition_val: &Value) -> bool {
    let Some(arr) = condition_val.as_array() else {
        return false;
    };
    match ctx_val {
        ContextValue::String(s) => {
            let s_lower = s.to_lowercase();
            arr.iter()
                .any(|v| v.as_str().is_some_and(|cv| cv.to_lowercase() == s_lower))
        }
        ContextValue::Int(i) => arr.iter().any(|v| v.as_i64().is_some_and(|cv| cv == *i)),
        ContextValue::Float(f) => arr.iter().any(|v| v.as_f64().is_some_and(|cv| cv == *f)),
        ContextValue::Bool(b) => arr.iter().any(|v| v.as_bool().is_some_and(|cv| cv == *b)),
        _ => false,
    }
}

fn eval_contains(ctx_val: &ContextValue, condition_val: &Value) -> bool {
    match ctx_val {
        ContextValue::StringList(list) => condition_val.as_str().is_some_and(|s| {
            let s_lower = s.to_lowercase();
            list.iter().any(|cv| cv.to_lowercase() == s_lower)
        }),
        ContextValue::IntList(list) => condition_val.as_i64().is_some_and(|i| list.contains(&i)),
        ContextValue::FloatList(list) => condition_val.as_f64().is_some_and(|f| list.contains(&f)),
        ContextValue::BoolList(list) => condition_val.as_bool().is_some_and(|b| list.contains(&b)),
        _ => false,
    }
}

fn eval_equals(ctx_val: &ContextValue, condition_val: &Value) -> bool {
    match ctx_val {
        ContextValue::String(s) => condition_val
            .as_str()
            .is_some_and(|cv| cv.to_lowercase() == s.to_lowercase()),
        ContextValue::Int(i) => condition_val.as_i64().is_some_and(|cv| cv == *i),
        ContextValue::Float(f) => condition_val.as_f64().is_some_and(|cv| cv == *f),
        ContextValue::Bool(b) => condition_val.as_bool().is_some_and(|cv| cv == *b),
        ContextValue::StringList(list) => condition_val.as_array().is_some_and(|arr| {
            arr.len() == list.len()
                && arr.iter().zip(list.iter()).all(|(cv, rv)| {
                    cv.as_str()
                        .is_some_and(|s| s.to_lowercase() == rv.to_lowercase())
                })
        }),
        ContextValue::IntList(list) => condition_val.as_array().is_some_and(|arr| {
            arr.len() == list.len()
                && arr
                    .iter()
                    .zip(list.iter())
                    .all(|(cv, rv)| cv.as_i64().is_some_and(|i| i == *rv))
        }),
        ContextValue::FloatList(list) => condition_val.as_array().is_some_and(|arr| {
            arr.len() == list.len()
                && arr
                    .iter()
                    .zip(list.iter())
                    .all(|(cv, rv)| cv.as_f64().is_some_and(|f| f == *rv))
        }),
        ContextValue::BoolList(list) => condition_val.as_array().is_some_and(|arr| {
            arr.len() == list.len()
                && arr
                    .iter()
                    .zip(list.iter())
                    .all(|(cv, rv)| cv.as_bool().is_some_and(|b| b == *rv))
        }),
    }
}

#[derive(Debug, PartialEq)]
enum DebugLogLevel {
    None,
    Parse,
    Match,
    All,
}

static DEBUG_LOG_LEVEL: OnceLock<DebugLogLevel> = OnceLock::new();
static DEBUG_MATCH_SAMPLE_RATE: OnceLock<u64> = OnceLock::new();

fn debug_log_level() -> &'static DebugLogLevel {
    DEBUG_LOG_LEVEL.get_or_init(|| {
        match std::env::var("SENTRY_OPTIONS_FEATURE_DEBUG_LOG")
            .as_deref()
            .unwrap_or("")
        {
            "all" => DebugLogLevel::All,
            "parse" => DebugLogLevel::Parse,
            "match" => DebugLogLevel::Match,
            _ => DebugLogLevel::None,
        }
    })
}

fn debug_match_sample_rate() -> u64 {
    *DEBUG_MATCH_SAMPLE_RATE.get_or_init(|| {
        std::env::var("SENTRY_OPTIONS_FEATURE_DEBUG_LOG_SAMPLE_RATE")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .map(|r| (r.clamp(0.0, 1.0) * 1000.0) as u64)
            .unwrap_or(1000)
    })
}

fn debug_log_parse(msg: &str) {
    match debug_log_level() {
        DebugLogLevel::Parse | DebugLogLevel::All => eprintln!("[sentry-options/parse] {msg}"),
        _ => {}
    }
}

fn debug_log_match(feature: &str, result: bool, context_id: u64) {
    match debug_log_level() {
        DebugLogLevel::Match | DebugLogLevel::All => {
            if context_id % 1000 < debug_match_sample_rate() {
                eprintln!(
                    "[sentry-options/match] feature='{feature}' result={result} context_id={context_id}"
                );
            }
        }
        _ => {}
    }
}

/// A handle for checking feature flags within a specific namespace.
pub struct FeatureChecker {
    namespace: String,
    options: &'static crate::Options,
}

impl FeatureChecker {
    pub fn new(namespace: String, options: &'static crate::Options) -> Self {
        Self { namespace, options }
    }

    /// Check whether a feature flag is enabled for a given context.
    ///
    /// Returns false if the feature is not defined, not enabled, or conditions don't match.
    pub fn has(&self, feature_name: &str, context: &FeatureContext) -> bool {
        let key = format!("feature.{feature_name}");

        let feature_val = match self.options.get(&self.namespace, &key) {
            Ok(v) => v,
            Err(e) => {
                debug_log_parse(&format!("Failed to get feature '{key}': {e}"));
                return false;
            }
        };

        let feature = match Feature::from_json(&feature_val) {
            Some(f) => {
                debug_log_parse(&format!("Parsed feature '{key}'"));
                f
            }
            None => {
                debug_log_parse(&format!("Failed to parse feature '{key}'"));
                return false;
            }
        };

        let result = feature.matches(context);
        debug_log_match(feature_name, result, context.id());
        result
    }
}

/// Get a feature checker handle for a namespace.
///
/// Panics if `init()` has not been called.
pub fn features(namespace: &str) -> FeatureChecker {
    let opts = crate::GLOBAL_OPTIONS
        .get()
        .expect("options not initialized - call init() first");
    FeatureChecker {
        namespace: namespace.to_string(),
        options: opts,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Options;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn create_schema(dir: &Path, namespace: &str, schema: &str) {
        let schema_dir = dir.join(namespace);
        fs::create_dir_all(&schema_dir).unwrap();
        fs::write(schema_dir.join("schema.json"), schema).unwrap();
    }

    fn create_values(dir: &Path, namespace: &str, values: &str) {
        let ns_dir = dir.join(namespace);
        fs::create_dir_all(&ns_dir).unwrap();
        fs::write(ns_dir.join("values.json"), values).unwrap();
    }

    const FEATURE_SCHEMA: &str = r##"{
        "version": "1.0",
        "type": "object",
        "properties": {
            "feature.organizations:test-feature": {
                "$ref": "#/definitions/Feature"
            }
        }
    }"##;

    fn setup_feature_options(feature_json: &str) -> (Options, TempDir) {
        let temp = TempDir::new().unwrap();
        let schemas = temp.path().join("schemas");
        fs::create_dir_all(&schemas).unwrap();
        create_schema(&schemas, "test", FEATURE_SCHEMA);

        let values = temp.path().join("values");
        let values_json = format!(
            r#"{{"options": {{"feature.organizations:test-feature": {}}}}}"#,
            feature_json
        );
        create_values(&values, "test", &values_json);

        let opts = Options::from_directory(temp.path()).unwrap();
        (opts, temp)
    }

    fn feature_json(enabled: bool, rollout: u64, conditions: &str) -> String {
        format!(
            r#"{{
                "name": "test-feature",
                "enabled": {enabled},
                "owner": {{"team": "test-team"}},
                "created_at": "2024-01-01",
                "segments": [{{
                    "name": "test-segment",
                    "rollout": {rollout},
                    "conditions": [{conditions}]
                }}]
            }}"#
        )
    }

    fn in_condition(property: &str, values: &str) -> String {
        format!(r#"{{"property": "{property}", "operator": "in", "value": [{values}]}}"#)
    }

    fn check(opts: &Options, feature: &str, ctx: &FeatureContext) -> bool {
        let key = format!("feature.{feature}");
        let Ok(val) = opts.get("test", &key) else {
            return false;
        };
        dbg!(&val);
        Feature::from_json(&val).is_some_and(|f| f.matches(ctx))
    }

    #[test]
    fn test_feature_context_insert_and_get() {
        let mut ctx = FeatureContext::new();
        ctx.insert("org_id", 123.into());
        ctx.insert("name", "sentry".into());
        ctx.insert("active", true.into());

        assert!(ctx.has("org_id"));
        assert!(!ctx.has("missing"));
        assert!(matches!(ctx.get("org_id"), Some(ContextValue::Int(123))));
        assert!(matches!(ctx.get("name"), Some(ContextValue::String(s)) if s == "sentry"));
    }

    #[test]
    fn test_feature_context_id_is_cached() {
        let mut ctx = FeatureContext::new();
        ctx.identity_fields(vec!["user_id"]);
        ctx.insert("user_id", 42.into());

        let id1 = ctx.id();
        let id2 = ctx.id();
        assert_eq!(id1, id2, "ID should be cached and consistent");
    }

    #[test]
    fn test_feature_context_id_resets_on_identity_change() {
        let mut ctx = FeatureContext::new();
        ctx.insert("user_id", 1.into());
        ctx.insert("org_id", 2.into());

        ctx.identity_fields(vec!["user_id"]);
        let id_user = ctx.id();

        ctx.identity_fields(vec!["org_id"]);
        let id_org = ctx.id();

        assert_ne!(
            id_user, id_org,
            "Different identity fields should produce different IDs"
        );
    }

    #[test]
    fn test_feature_context_id_deterministic() {
        let make_ctx = || {
            let mut ctx = FeatureContext::new();
            ctx.identity_fields(vec!["user_id", "org_id"]);
            ctx.insert("user_id", 456.into());
            ctx.insert("org_id", 123.into());
            ctx
        };

        assert_eq!(make_ctx().id(), make_ctx().id());

        let mut other_ctx = FeatureContext::new();
        other_ctx.identity_fields(vec!["user_id", "org_id"]);
        other_ctx.insert("user_id", 789.into());
        other_ctx.insert("org_id", 123.into());

        assert_ne!(make_ctx().id(), other_ctx.id());
    }

    #[test]
    fn test_feature_context_id_value_align_with_python() {
        // Context.id() determines rollout rates with modulo
        // This implementation should generate the same rollout slots
        // as the previous implementation did.
        let ctx = FeatureContext::new();
        assert_eq!(ctx.id() % 100, 5, "should match with python implementation");

        let mut ctx = FeatureContext::new();
        ctx.insert("foo", "bar".into());
        ctx.insert("baz", "barfoo".into());
        ctx.identity_fields(vec!["foo"]);
        assert_eq!(ctx.id() % 100, 62);

        // Undefined fields should not contribute to the id.
        let mut ctx = FeatureContext::new();
        ctx.insert("foo", "bar".into());
        ctx.insert("baz", "barfoo".into());
        ctx.identity_fields(vec!["foo", "whoops"]);
        assert_eq!(ctx.id() % 100, 62);

        let mut ctx = FeatureContext::new();
        ctx.insert("foo", "bar".into());
        ctx.insert("baz", "barfoo".into());
        ctx.identity_fields(vec!["foo", "baz"]);
        assert_eq!(ctx.id() % 100, 1);

        let mut ctx = FeatureContext::new();
        ctx.insert("foo", "bar".into());
        ctx.insert("baz", "barfoo".into());
        // When there is no overlap with identity fields and data,
        // all fields should be used
        ctx.identity_fields(vec!["whoops", "nope"]);
        assert_eq!(ctx.id() % 100, 1);
    }

    #[test]
    fn test_feature_prefix_is_added() {
        let cond = in_condition("organization_id", "123");
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, &cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("organization_id", 123.into());

        assert!(check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_undefined_feature_returns_false() {
        let cond = in_condition("organization_id", "123");
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, &cond));

        let ctx = FeatureContext::new();
        assert!(!check(&opts, "nonexistent", &ctx));
    }

    #[test]
    fn test_missing_context_field_returns_false() {
        let cond = in_condition("organization_id", "123");
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, &cond));

        let ctx = FeatureContext::new();
        assert!(!check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_matching_context_returns_true() {
        let cond = in_condition("organization_id", "123, 456");
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, &cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("organization_id", 123.into());

        assert!(check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_non_matching_context_returns_false() {
        let cond = in_condition("organization_id", "123, 456");
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, &cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("organization_id", 999.into());

        assert!(!check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_disabled_feature_returns_false() {
        let cond = in_condition("organization_id", "123");
        let (opts, _t) = setup_feature_options(&feature_json(false, 100, &cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("organization_id", 123.into());

        assert!(!check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_rollout_zero_returns_false() {
        let cond = in_condition("organization_id", "123");
        let (opts, _t) = setup_feature_options(&feature_json(true, 0, &cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("organization_id", 123.into());

        assert!(!check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_rollout_100_returns_true() {
        let cond = in_condition("organization_id", "123");
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, &cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("organization_id", 123.into());

        assert!(check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_rollout_is_deterministic() {
        let mut ctx = FeatureContext::new();
        ctx.identity_fields(vec!["user_id"]);
        ctx.insert("user_id", 42.into());
        ctx.insert("organization_id", 123.into());

        // Add 1 to get around fence post with < vs <=
        let id_mod = (ctx.id() % 100) + 1;
        let cond = in_condition("organization_id", "123");

        let (opts_at, _t1) = setup_feature_options(&feature_json(true, id_mod, &cond));
        assert!(check(&opts_at, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_condition_in_string_case_insensitive() {
        let cond = r#"{"property": "slug", "operator": "in", "value": ["Sentry", "ACME"]}"#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("slug", "sentry".into());
        assert!(check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_condition_not_in() {
        let cond = r#"{"property": "organization_id", "operator": "not_in", "value": [999]}"#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("organization_id", 123.into());
        assert!(check(&opts, "organizations:test-feature", &ctx));

        let mut ctx2 = FeatureContext::new();
        ctx2.insert("organization_id", 999.into());
        assert!(!check(&opts, "organizations:test-feature", &ctx2));
    }

    #[test]
    fn test_condition_contains() {
        let cond = r#"{"property": "tags", "operator": "contains", "value": "beta"}"#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, cond));

        let mut ctx = FeatureContext::new();
        ctx.insert(
            "tags",
            ContextValue::StringList(vec!["alpha".into(), "beta".into()]),
        );
        assert!(check(&opts, "organizations:test-feature", &ctx));

        let mut ctx2 = FeatureContext::new();
        ctx2.insert("tags", ContextValue::StringList(vec!["alpha".into()]));
        assert!(!check(&opts, "organizations:test-feature", &ctx2));
    }

    #[test]
    fn test_condition_equals() {
        let cond = r#"{"property": "plan", "operator": "equals", "value": "enterprise"}"#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("plan", "Enterprise".into());
        assert!(check(&opts, "organizations:test-feature", &ctx));

        let mut ctx2 = FeatureContext::new();
        ctx2.insert("plan", "free".into());
        assert!(!check(&opts, "organizations:test-feature", &ctx2));
    }

    #[test]
    fn test_condition_equals_bool() {
        let cond = r#"{"property": "is_free", "operator": "equals", "value": true}"#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("is_free", true.into());
        assert!(check(&opts, "organizations:test-feature", &ctx));

        let mut ctx2 = FeatureContext::new();
        ctx2.insert("is_free", false.into());
        assert!(!check(&opts, "organizations:test-feature", &ctx2));
    }

    #[test]
    fn test_segment_with_no_conditions_always_matches() {
        let feature = r#"{
            "name": "test-feature",
            "enabled": true,
            "owner": {"team": "test-team"},
            "created_at": "2024-01-01",
            "segments": [{"name": "open", "rollout": 100, "conditions": []}]
        }"#;
        let (opts, _t) = setup_feature_options(feature);

        let ctx = FeatureContext::new();
        assert!(check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_feature_with_no_segments_returns_false() {
        let feature = r#"{
            "name": "test-feature",
            "enabled": true,
            "owner": {"team": "test-team"},
            "created_at": "2024-01-01",
            "segments": []
        }"#;
        let (opts, _t) = setup_feature_options(feature);

        let ctx = FeatureContext::new();
        assert!(!check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_multiple_segments_or_logic() {
        let feature = r#"{
            "name": "test-feature",
            "enabled": true,
            "owner": {"team": "test-team"},
            "created_at": "2024-01-01",
            "segments": [
                {
                    "name": "segment-a",
                    "rollout": 100,
                    "conditions": [{"property": "org_id", "operator": "in", "value": [1]}]
                },
                {
                    "name": "segment-b",
                    "rollout": 100,
                    "conditions": [{"property": "org_id", "operator": "in", "value": [2]}]
                }
            ]
        }"#;
        let (opts, _t) = setup_feature_options(feature);

        let mut ctx1 = FeatureContext::new();
        ctx1.insert("org_id", 1.into());
        assert!(check(&opts, "organizations:test-feature", &ctx1));

        let mut ctx2 = FeatureContext::new();
        ctx2.insert("org_id", 2.into());
        assert!(check(&opts, "organizations:test-feature", &ctx2));

        let mut ctx3 = FeatureContext::new();
        ctx3.insert("org_id", 3.into());
        assert!(!check(&opts, "organizations:test-feature", &ctx3));
    }

    #[test]
    fn test_multiple_conditions_and_logic() {
        let conds = r#"
            {"property": "org_id", "operator": "in", "value": [123]},
            {"property": "user_email", "operator": "in", "value": ["admin@example.com"]}
        "#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, conds));

        let mut ctx = FeatureContext::new();
        ctx.insert("org_id", 123.into());
        ctx.insert("user_email", "admin@example.com".into());
        assert!(check(&opts, "organizations:test-feature", &ctx));

        let mut ctx2 = FeatureContext::new();
        ctx2.insert("org_id", 123.into());
        ctx2.insert("user_email", "other@example.com".into());
        assert!(!check(&opts, "organizations:test-feature", &ctx2));
    }

    #[test]
    fn test_in_int_context_against_string_list_returns_false() {
        let cond = r#"{"property": "org_id", "operator": "in", "value": ["123", "456"]}"#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("org_id", 123.into());
        assert!(!check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_in_string_context_against_int_list_returns_false() {
        let cond = r#"{"property": "slug", "operator": "in", "value": [123, 456]}"#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("slug", "123".into());
        assert!(!check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_in_bool_context_against_string_list_returns_false() {
        let cond = r#"{"property": "active", "operator": "in", "value": ["true", "false"]}"#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("active", true.into());
        assert!(!check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_in_float_context_against_string_list_returns_false() {
        let cond = r#"{"property": "score", "operator": "in", "value": ["0.5", "1.0"]}"#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("score", 0.5.into());
        assert!(!check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_not_in_int_context_against_string_list_returns_true() {
        // Type mismatch means "in" is false, so "not_in" is true
        let cond = r#"{"property": "org_id", "operator": "not_in", "value": ["123", "456"]}"#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("org_id", 123.into());
        assert!(check(&opts, "organizations:test-feature", &ctx));
    }

    #[test]
    fn test_not_in_string_context_against_int_list_returns_true() {
        // Type mismatch means "in" is false, so "not_in" is true
        let cond = r#"{"property": "slug", "operator": "not_in", "value": [123, 456]}"#;
        let (opts, _t) = setup_feature_options(&feature_json(true, 100, cond));

        let mut ctx = FeatureContext::new();
        ctx.insert("slug", "123".into());
        assert!(check(&opts, "organizations:test-feature", &ctx));
    }
}
