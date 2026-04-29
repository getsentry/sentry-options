//! Options client for reading validated configuration values.

pub mod features;

pub use features::{FeatureChecker, FeatureContext, features};

use arc_swap::ArcSwap;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use sentry_options_validation::{
    SchemaRegistry, ValidationError, ValuesWatcher, resolve_options_dir,
};
use serde_json::Value;
use thiserror::Error;

pub mod testing;

static GLOBAL_OPTIONS: OnceLock<Options> = OnceLock::new();

#[derive(Debug, Error)]
pub enum OptionsError {
    #[error("Options not initialized - call init() first")]
    NotInitialized,

    #[error("Unknown namespace: {0}")]
    UnknownNamespace(String),

    #[error("Unknown option '{key}' in namespace '{namespace}'")]
    UnknownOption { namespace: String, key: String },

    #[error("Schema error: {0}")]
    Schema(#[from] ValidationError),
}

pub type Result<T> = std::result::Result<T, OptionsError>;

/// Options store for reading configuration values.
pub struct Options {
    registry: Arc<SchemaRegistry>,
    values: Arc<ArcSwap<HashMap<String, HashMap<String, Value>>>>,
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
        let registry = SchemaRegistry::from_directory(&base_dir.join("schemas"))?;
        Self::with_registry_and_values(registry, &base_dir.join("values"))
    }

    /// Load options with schemas provided as in-memory JSON strings.
    /// Values are loaded from disk using the standard fallback chain.
    pub fn from_schemas(schemas: &[(&str, &str)]) -> Result<Self> {
        let registry = SchemaRegistry::from_schemas(schemas)?;
        Self::with_registry_and_values(registry, &resolve_options_dir().join("values"))
    }

    fn with_registry_and_values(registry: SchemaRegistry, values_dir: &Path) -> Result<Self> {
        let registry = Arc::new(registry);
        let (loaded_values, _) = registry.load_values_json(values_dir)?;
        let values = Arc::new(ArcSwap::from_pointee(loaded_values));
        let watcher = ValuesWatcher::new(values_dir, Arc::clone(&registry), Arc::clone(&values))?;
        Ok(Self {
            registry,
            values,
            _watcher: watcher,
        })
    }

    /// Get an option value, returning the schema default if not set.
    pub fn get(&self, namespace: &str, key: &str) -> Result<Value> {
        if let Some(value) = testing::get_override(namespace, key) {
            return Ok(value);
        }

        let schema = self
            .registry
            .get(namespace)
            .ok_or_else(|| OptionsError::UnknownNamespace(namespace.to_string()))?;

        let values_guard = self.values.load();
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

    /// Validate that a key exists in the schema and the value matches the expected type.
    pub fn validate_override(&self, namespace: &str, key: &str, value: &Value) -> Result<()> {
        let schema = self
            .registry
            .get(namespace)
            .ok_or_else(|| OptionsError::UnknownNamespace(namespace.to_string()))?;

        schema.validate_option(key, value)?;

        Ok(())
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

        let values_guard = self.values.load();
        if let Some(ns_values) = values_guard.get(namespace) {
            Ok(ns_values.contains_key(key))
        } else {
            Ok(false)
        }
    }
}

/// Initialize global options using fallback chain: `SENTRY_OPTIONS_DIR` env var,
/// then `/etc/sentry-options` if it exists, otherwise `sentry-options/`.
///
/// Idempotent: if already initialized, returns `Ok(())` without re-loading.
pub fn init() -> Result<()> {
    if GLOBAL_OPTIONS.get().is_some() {
        return Ok(());
    }
    let opts = Options::new()?;
    let _ = GLOBAL_OPTIONS.set(opts);
    Ok(())
}

/// Initialize global options with schemas provided as in-memory JSON strings.
/// Values are loaded from disk using the standard fallback chain.
///
/// Idempotent: if already initialized (by `init()` or a prior `init_with_schemas()`),
/// returns `Ok(())` without updating schemas.
///
/// Use this when schemas are embedded in the binary via `include_str!`:
/// ```rust,ignore
/// init_with_schemas(&[
///     ("snuba", include_str!("sentry-options/schemas/snuba/schema.json")),
/// ])?;
/// ```
pub fn init_with_schemas(schemas: &[(&str, &str)]) -> Result<()> {
    if GLOBAL_OPTIONS.get().is_some() {
        return Ok(());
    }
    let opts = Options::from_schemas(schemas)?;
    let _ = GLOBAL_OPTIONS.set(opts);
    Ok(())
}

/// Get a namespace handle for accessing options.
///
/// Returns an error if `init()` has not been called.
pub fn options(namespace: &str) -> Result<NamespaceOptions> {
    let opts = GLOBAL_OPTIONS.get().ok_or(OptionsError::NotInitialized)?;
    Ok(NamespaceOptions {
        namespace: namespace.to_string(),
        options: opts,
    })
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

    #[test]
    fn test_from_schemas_get_default() {
        let schema = r#"{
            "version": "1.0",
            "type": "object",
            "properties": {
                "enabled": {
                    "type": "boolean",
                    "default": false,
                    "description": "Enable feature"
                }
            }
        }"#;

        let registry = SchemaRegistry::from_schemas(&[("test", schema)]).unwrap();
        let default = registry
            .get("test")
            .unwrap()
            .get_default("enabled")
            .unwrap();
        assert_eq!(*default, json!(false));
    }

    #[test]
    fn test_from_schemas_with_values() {
        let temp = TempDir::new().unwrap();
        let values_dir = temp.path().join("values");
        create_values(&values_dir, "test", r#"{"options": {"enabled": true}}"#);

        let schema = r#"{
            "version": "1.0",
            "type": "object",
            "properties": {
                "enabled": {
                    "type": "boolean",
                    "default": false,
                    "description": "Enable feature"
                }
            }
        }"#;

        let registry = Arc::new(SchemaRegistry::from_schemas(&[("test", schema)]).unwrap());
        let (loaded_values, _) = registry.load_values_json(&values_dir).unwrap();
        assert_eq!(loaded_values["test"]["enabled"], json!(true));
    }

    #[test]
    fn test_from_schemas_invalid_json() {
        let result = SchemaRegistry::from_schemas(&[("test", "not valid json")]);
        assert!(result.is_err());
    }
}
