//! Options client for reading validated configuration values.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};

use sentry_options_validation::{SchemaRegistry, ValidationError, ValuesWatcher};
use serde_json::Value;
use thiserror::Error;

const DEFAULT_OPTIONS_DIR: &str = "/etc/sentry-options";
const OPTIONS_DIR_ENV: &str = "SENTRY_OPTIONS_DIR";

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
    /// Load options from default path (`/etc/sentry-options`) or `SENTRY_OPTIONS_DIR` env var.
    /// Expects `{dir}/schemas/` and `{dir}/values/` subdirectories.
    pub fn new() -> Result<Self> {
        let base_dir = std::env::var(OPTIONS_DIR_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_OPTIONS_DIR));

        Self::from_directory(&base_dir)
    }

    /// Load options from a specific directory (useful for testing).
    /// Expects `{base_dir}/schemas/` and `{base_dir}/values/` subdirectories.
    pub fn from_directory(base_dir: &Path) -> Result<Self> {
        let schemas_dir = base_dir.join("schemas");
        let values_dir = base_dir.join("values");

        let registry = Arc::new(SchemaRegistry::from_directory(&schemas_dir)?);
        let loaded_values = registry.load_values_json(&values_dir)?;
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
}

/// Initialize global options from default path or `SENTRY_OPTIONS_DIR` env var.
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
        create_values(&values, "test", r#"{"enabled": true}"#);

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
}
