//! Options client for reading validated configuration values.

pub mod features;
pub use features::{ContextValue, FeatureContext};

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

use sentry_options_validation::{
    SchemaRegistry, ValidationError, ValuesWatcher, resolve_options_dir,
};
use serde_json::Value;
use thiserror::Error;

static GLOBAL_OPTIONS: OnceLock<Options> = OnceLock::new();

#[derive(Debug, Error)]
pub enum OptionsError {
    #[error("Unknown namespace: {0}")]
    UnknownNamespace(String),

    #[error("Unknown option '{key}' in namespace '{namespace}'")]
    UnknownOption { namespace: String, key: String },

    #[error("Schema error: {0}")]
    Schema(#[from] ValidationError),

    #[error("Options already initialized")]
    AlreadyInitialized,
}

pub type Result<T> = std::result::Result<T, OptionsError>;

/// Options store for reading configuration values.
pub struct Options {
    registry: Arc<SchemaRegistry>,
    values: Arc<RwLock<HashMap<String, HashMap<String, Value>>>>,
    _watcher: ValuesWatcher,
}

impl Options {
    /// Load options using fallback chain: `SENTRY_OPTIONS_DIR` env var, then `/etc/sentry-options`
    /// if it exists, otherwise `sentry-options/`.
    /// Expects `{dir}/schemas/` and `{dir}/values/` subdirectories.
    pub fn new() -> Result<Self> {
        Self::from_directory(&resolve_options_dir())
    }

    /// Load options from a specific directory (useful for testing).
    /// Expects `{base_dir}/schemas/` and `{base_dir}/values/` subdirectories.
    pub fn from_directory(base_dir: &Path) -> Result<Self> {
        let schemas_dir = base_dir.join("schemas");
        let values_dir = base_dir.join("values");

        let registry = Arc::new(SchemaRegistry::from_directory(&schemas_dir)?);
        let (loaded_values, _) = registry.load_values_json(&values_dir)?;
        let values = Arc::new(RwLock::new(loaded_values));

        let watcher_registry = Arc::clone(&registry);
        let watcher_values = Arc::clone(&values);
        // will automatically stop thread when dropped out of scope
        let watcher = ValuesWatcher::new(values_dir.as_path(), watcher_registry, watcher_values)?;

        Ok(Self {
            registry,
            values,
            _watcher: watcher,
        })
    }

    /// Get an option value, returning the schema default if not set.
    pub fn get(&self, namespace: &str, key: &str) -> Result<Value> {
        let schema = self
            .registry
            .get(namespace)
            .ok_or_else(|| OptionsError::UnknownNamespace(namespace.to_string()))?;

        let values_guard = self
            .values
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(ns_values) = values_guard.get(namespace)
            && let Some(value) = ns_values.get(key)
        {
            return Ok(value.clone());
        }

        let default = schema
            .get_default(key)
            .ok_or_else(|| OptionsError::UnknownOption {
                namespace: namespace.to_string(),
                key: key.to_string(),
            })?;

        Ok(default.clone())
    }

    /// Check if an option has a value.
    ///
    /// Returns true if the option is defined and has a value, will return
    /// false if the option is defined and does not have a value.
    ///
    /// If the namespace or option are not defined, an Err will be returned.
    pub fn isset(&self, namespace: &str, key: &str) -> Result<bool> {
        let schema = self
            .registry
            .get(namespace)
            .ok_or_else(|| OptionsError::UnknownNamespace(namespace.to_string()))?;

        if !schema.options.contains_key(key) {
            return Err(OptionsError::UnknownOption {
                namespace: namespace.into(),
                key: key.into(),
            });
        }

        let values_guard = self
            .values
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if let Some(ns_values) = values_guard.get(namespace) {
            Ok(ns_values.contains_key(key))
        } else {
            Ok(false)
        }
    }
}

/// Initialize global options using fallback chain: `SENTRY_OPTIONS_DIR` env var,
/// then `/etc/sentry-options` if it exists, otherwise `sentry-options/`.
pub fn init() -> Result<()> {
    let opts = Options::new()?;
    GLOBAL_OPTIONS
        .set(opts)
        .map_err(|_| OptionsError::AlreadyInitialized)?;
    Ok(())
}

/// Get a namespace handle for accessing options.
///
/// Panics if `init()` has not been called.
pub fn options(namespace: &str) -> NamespaceOptions {
    let opts = GLOBAL_OPTIONS
        .get()
        .expect("options not initialized - call init() first");
    NamespaceOptions {
        namespace: namespace.to_string(),
        options: opts,
    }
}

/// Get a feature checker for a specific namespace.
///
/// Panics if `init()` has not been called.
pub fn features(namespace: &str) -> FeatureChecker {
    let opts = GLOBAL_OPTIONS
        .get()
        .expect("options not initialized - call init() first");
    FeatureChecker {
        namespace: namespace.to_string(),
        options: opts,
    }
}

/// Handle for evaluating feature flags within a specific namespace.
pub struct FeatureChecker {
    namespace: String,
    options: &'static Options,
}

impl FeatureChecker {
    /// Create a `FeatureChecker` bound to the given namespace and options instance.
    pub fn new(namespace: impl Into<String>, options: &'static Options) -> Self {
        Self {
            namespace: namespace.into(),
            options,
        }
    }

    /// Check whether `feature_name` is enabled for the given context.
    ///
    /// Loads the feature config from `features.{feature_name}` in the namespace,
    /// parses it, and evaluates it against `context`. All errors return `false`.
    pub fn has(&self, feature_name: &str, context: &FeatureContext) -> bool {
        let key = format!("features.{feature_name}");
        let value = match self.options.get(&self.namespace, &key) {
            Ok(v) => v,
            Err(_) => return false,
        };
        let json_str = match value.as_str() {
            Some(s) => s.to_string(),
            None => return false,
        };

        let cfg = features::debug_config();
        let feature_data: features::FeatureData = match serde_json::from_str(&json_str) {
            Ok(d) => d,
            Err(e) => {
                if cfg.log_parse {
                    eprintln!(
                        "[sentry-options] failed to parse feature '{}': {e}",
                        feature_name
                    );
                }
                return false;
            }
        };
        feature_data.evaluate(feature_name, context)
    }
}

/// Handle for accessing options within a specific namespace.
pub struct NamespaceOptions {
    namespace: String,
    options: &'static Options,
}

impl NamespaceOptions {
    /// Get an option value, returning the schema default if not set.
    pub fn get(&self, key: &str) -> Result<Value> {
        self.options.get(&self.namespace, key)
    }

    /// Check if an option has a key defined, or if the default is being used.
    pub fn isset(&self, key: &str) -> Result<bool> {
        self.options.isset(&self.namespace, key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    // FeatureChecker helpers

    /// Leak an Options instance to satisfy the `&'static Options` requirement.
    ///
    /// This is acceptable in tests since the leaked memory is reclaimed when the
    /// process exits.
    fn leak_options(opts: Options) -> &'static Options {
        Box::leak(Box::new(opts))
    }

    fn make_checker(opts: &'static Options) -> FeatureChecker {
        FeatureChecker {
            namespace: "test".to_string(),
            options: opts,
        }
    }

    /// Build a FeatureChecker with `feature_name` registered in the schema and
    /// `feature_json` stored as its value. Returns the TempDir to keep it alive.
    fn checker_with_feature(feature_name: &str, feature_json: &str) -> (TempDir, FeatureChecker) {
        let temp = TempDir::new().unwrap();
        let schemas = temp.path().join("schemas");
        let values = temp.path().join("values");
        fs::create_dir_all(&schemas).unwrap();
        let schema = format!(
            r#"{{"version":"1.0","type":"object","properties":{{"features.{feature_name}":{{"type":"string","default":"","description":"Feature flag config"}}}}}}"#
        );
        create_schema(&schemas, "test", &schema);
        create_values(
            &values,
            "test",
            &format!(
                r#"{{"options":{{"features.{feature_name}":{}}}}}"#,
                serde_json::to_string(feature_json).unwrap()
            ),
        );
        let checker = make_checker(leak_options(Options::from_directory(temp.path()).unwrap()));
        (temp, checker)
    }

    // FeatureChecker tests

    #[test]
    fn test_has_unknown_feature_returns_false() {
        let temp = TempDir::new().unwrap();
        let schemas = temp.path().join("schemas");
        fs::create_dir_all(&schemas).unwrap();
        // Schema with no feature keys
        create_schema(
            &schemas,
            "test",
            r#"{"version": "1.0", "type": "object", "properties": {}}"#,
        );
        let checker = make_checker(leak_options(Options::from_directory(temp.path()).unwrap()));
        let ctx = FeatureContext::new();
        assert!(!checker.has("organizations:fury-mode", &ctx));
    }

    #[test]
    fn test_has_disabled_feature_returns_false() {
        let feature_json = r#"{"enabled": false, "segments": [{"name": "all", "rollout": 100, "conditions": []}]}"#;
        let (_temp, checker) = checker_with_feature("organizations:fury-mode", feature_json);
        let ctx = FeatureContext::new();
        assert!(!checker.has("organizations:fury-mode", &ctx));
    }

    #[test]
    fn test_has_matching_context_returns_true() {
        let feature_json = r#"{"enabled": true, "segments": [{"name": "sentry orgs", "rollout": 100, "conditions": [{"property": "organization_slug", "operator": {"kind": "in", "value": ["sentry", "sentry-test"]}}]}]}"#;
        let (_temp, checker) = checker_with_feature("organizations:fury-mode", feature_json);
        let mut ctx = FeatureContext::new();
        ctx.insert("organization_slug", "sentry".into());
        assert!(checker.has("organizations:fury-mode", &ctx));
    }

    #[test]
    fn test_has_missing_context_field_returns_false() {
        let feature_json = r#"{"enabled": true, "segments": [{"name": "sentry orgs", "rollout": 100, "conditions": [{"property": "organization_slug", "operator": {"kind": "in", "value": ["sentry"]}}]}]}"#;
        let (_temp, checker) = checker_with_feature("organizations:fury-mode", feature_json);
        let ctx = FeatureContext::new(); // no organization_slug
        assert!(!checker.has("organizations:fury-mode", &ctx));
    }

    #[test]
    fn test_has_invalid_operator_kind_returns_false() {
        // "bananas" is not a valid OperatorKind — serde_json will fail to parse
        let feature_json = r#"{"enabled": true, "segments": [{"name": "all", "rollout": 100, "conditions": [{"property": "organization_slug", "operator": {"kind": "bananas", "value": ["sentry"]}}]}]}"#;
        let (_temp, checker) = checker_with_feature("organizations:fury-mode", feature_json);
        let ctx = FeatureContext::new();
        assert!(!checker.has("organizations:fury-mode", &ctx));
    }

    #[test]
    fn test_has_invalid_segment_rollout_returns_false() {
        // rollout: 500 exceeds u8 max (255) — serde_json will fail to parse
        let feature_json =
            r#"{"enabled": true, "segments": [{"name": "all", "rollout": 500, "conditions": []}]}"#;
        let (_temp, checker) = checker_with_feature("organizations:fury-mode", feature_json);
        let ctx = FeatureContext::new();
        assert!(!checker.has("organizations:fury-mode", &ctx));
    }

    #[test]
    fn test_has_invalid_condition_missing_property_returns_false() {
        // condition is missing required "property" field — serde_json will fail to parse
        let feature_json = r#"{"enabled": true, "segments": [{"name": "all", "rollout": 100, "conditions": [{"operator": {"kind": "in", "value": ["sentry"]}}]}]}"#;
        let (_temp, checker) = checker_with_feature("organizations:fury-mode", feature_json);
        let ctx = FeatureContext::new();
        assert!(!checker.has("organizations:fury-mode", &ctx));
    }

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

    #[test]
    fn test_get_value() {
        let temp = TempDir::new().unwrap();
        let schemas = temp.path().join("schemas");
        let values = temp.path().join("values");
        fs::create_dir_all(&schemas).unwrap();

        create_schema(
            &schemas,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "enabled": {
                        "type": "boolean",
                        "default": false,
                        "description": "Enable feature"
                    }
                }
            }"#,
        );
        create_values(&values, "test", r#"{"options": {"enabled": true}}"#);

        let options = Options::from_directory(temp.path()).unwrap();
        assert_eq!(options.get("test", "enabled").unwrap(), json!(true));
    }

    #[test]
    fn test_get_default() {
        let temp = TempDir::new().unwrap();
        let schemas = temp.path().join("schemas");
        let values = temp.path().join("values");
        fs::create_dir_all(&schemas).unwrap();
        fs::create_dir_all(&values).unwrap();

        create_schema(
            &schemas,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "timeout": {
                        "type": "integer",
                        "default": 30,
                        "description": "Timeout"
                    }
                }
            }"#,
        );

        let options = Options::from_directory(temp.path()).unwrap();
        assert_eq!(options.get("test", "timeout").unwrap(), json!(30));
    }

    #[test]
    fn test_unknown_namespace() {
        let temp = TempDir::new().unwrap();
        let schemas = temp.path().join("schemas");
        let values = temp.path().join("values");
        fs::create_dir_all(&schemas).unwrap();
        fs::create_dir_all(&values).unwrap();

        create_schema(
            &schemas,
            "test",
            r#"{"version": "1.0", "type": "object", "properties": {}}"#,
        );

        let options = Options::from_directory(temp.path()).unwrap();
        assert!(matches!(
            options.get("unknown", "key"),
            Err(OptionsError::UnknownNamespace(_))
        ));
    }

    #[test]
    fn test_unknown_option() {
        let temp = TempDir::new().unwrap();
        let schemas = temp.path().join("schemas");
        let values = temp.path().join("values");
        fs::create_dir_all(&schemas).unwrap();
        fs::create_dir_all(&values).unwrap();

        create_schema(
            &schemas,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "known": {"type": "string", "default": "x", "description": "Known"}
                }
            }"#,
        );

        let options = Options::from_directory(temp.path()).unwrap();
        assert!(matches!(
            options.get("test", "unknown"),
            Err(OptionsError::UnknownOption { .. })
        ));
    }

    #[test]
    fn test_missing_values_dir() {
        let temp = TempDir::new().unwrap();
        let schemas = temp.path().join("schemas");
        fs::create_dir_all(&schemas).unwrap();

        create_schema(
            &schemas,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "opt": {"type": "string", "default": "default_val", "description": "Opt"}
                }
            }"#,
        );

        let options = Options::from_directory(temp.path()).unwrap();
        assert_eq!(options.get("test", "opt").unwrap(), json!("default_val"));
    }

    #[test]
    fn isset_with_defined_and_undefined_keys() {
        let temp = TempDir::new().unwrap();
        let schemas = temp.path().join("schemas");
        fs::create_dir_all(&schemas).unwrap();

        let values = temp.path().join("values");
        create_values(&values, "test", r#"{"options": {"has-value": "yes"}}"#);

        create_schema(
            &schemas,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "has-value": {"type": "string", "default": "", "description": ""},
                    "defined-with-default": {"type": "string", "default": "default_val", "description": "Opt"}
                }
            }"#,
        );

        let options = Options::from_directory(temp.path()).unwrap();
        assert!(options.isset("test", "not-defined").is_err());
        assert!(!options.isset("test", "defined-with-default").unwrap());
        assert!(options.isset("test", "has-value").unwrap());
    }
}
