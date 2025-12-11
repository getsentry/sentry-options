//! Schema validation library for sentry-options
//!
//! This library provides schema loading and validation for sentry-options.
//! Schemas are loaded once and stored in Arc for efficient sharing.
//! Values are validated against schemas as complete objects.

use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Embedded meta-schema for validating sentry-options schema files
const NAMESPACE_SCHEMA_JSON: &str = include_str!("namespace-schema.json");
const SCHEMA_FILE_NAME: &str = "schema.json";
const VALUES_FILE_NAME: &str = "values.json";

/// Time between file polls in seconds
const POLLING_DELAY: u64 = 5;

/// Result type for validation operations
pub type ValidationResult<T> = Result<T, ValidationError>;

/// A map of option values keyed by their namespace
pub type ValuesByNamespace = HashMap<String, HashMap<String, Value>>;

/// Errors that can occur during schema and value validation
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Schema error in {file}: {message}")]
    SchemaError { file: PathBuf, message: String },

    #[error("Value error for {namespace}: {errors}")]
    ValueError { namespace: String, errors: String },

    #[error("Unknown namespace: {0}")]
    UnknownNamespace(String),

    #[error("Internal error: {0}")]
    InternalError(String),

    #[error("Failed to read file: {0}")]
    FileRead(#[from] std::io::Error),

    #[error("Failed to parse JSON: {0}")]
    JSONParse(#[from] serde_json::Error),
}

/// Schema for a namespace, containing validator and defaults
pub struct NamespaceSchema {
    pub namespace: String,
    defaults: HashMap<String, Value>,
    validator: jsonschema::Validator,
}

impl NamespaceSchema {
    /// Validate an entire values object against this schema
    ///
    /// # Arguments
    /// * `values` - JSON object containing option key-value pairs
    ///
    /// # Errors
    /// Returns error if values don't match the schema
    pub fn validate_values(&self, values: &Value) -> ValidationResult<()> {
        let output = self.validator.evaluate(values);
        if output.flag().valid {
            Ok(())
        } else {
            let errors: Vec<String> = output.iter_errors().map(|e| e.error.to_string()).collect();
            Err(ValidationError::ValueError {
                namespace: self.namespace.clone(),
                errors: errors.join(", "),
            })
        }
    }

    /// Get the default value for an option key.
    /// Returns None if the key doesn't exist in the schema.
    pub fn get_default(&self, key: &str) -> Option<&Value> {
        self.defaults.get(key)
    }
}

/// Registry for loading and storing schemas
pub struct SchemaRegistry {
    schemas: HashMap<String, Arc<NamespaceSchema>>,
}

impl SchemaRegistry {
    /// Create a new empty schema registry
    pub fn new() -> Self {
        Self {
            schemas: HashMap::new(),
        }
    }

    /// Load schemas from a directory and create a registry
    ///
    /// Expects directory structure: `schemas/{namespace}/schema.json`
    ///
    /// # Arguments
    /// * `schemas_dir` - Path to directory containing namespace subdirectories
    ///
    /// # Errors
    /// Returns error if directory doesn't exist or any schema is invalid
    pub fn from_directory(schemas_dir: &Path) -> ValidationResult<Self> {
        let schemas = Self::load_all_schemas(schemas_dir)?;
        Ok(Self { schemas })
    }

    /// Validate an entire values object for a namespace
    ///
    /// # Arguments
    /// * `namespace` - Namespace name
    /// * `values` - JSON object containing option key-value pairs
    ///
    /// # Errors
    /// Returns error if namespace doesn't exist or values don't match schema
    pub fn validate_values(&self, namespace: &str, values: &Value) -> ValidationResult<()> {
        let schema = self
            .schemas
            .get(namespace)
            .ok_or_else(|| ValidationError::UnknownNamespace(namespace.to_string()))?;

        schema.validate_values(values)
    }

    /// Load all schemas from a directory
    fn load_all_schemas(
        schemas_dir: &Path,
    ) -> ValidationResult<HashMap<String, Arc<NamespaceSchema>>> {
        // Compile namespace-schema once for all schemas
        let namespace_schema_value: Value =
            serde_json::from_str(NAMESPACE_SCHEMA_JSON).map_err(|e| {
                ValidationError::InternalError(format!("Invalid namespace-schema JSON: {}", e))
            })?;
        let namespace_validator =
            jsonschema::validator_for(&namespace_schema_value).map_err(|e| {
                ValidationError::InternalError(format!("Failed to compile namespace-schema: {}", e))
            })?;

        let mut schemas = HashMap::new();

        // TODO: Parallelize the loading of schemas for the performance gainz
        for entry in fs::read_dir(schemas_dir)? {
            let entry = entry?;

            if !entry.file_type()?.is_dir() {
                continue;
            }

            let namespace =
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| ValidationError::SchemaError {
                        file: entry.path(),
                        message: "Directory name contains invalid UTF-8".to_string(),
                    })?;

            let schema_file = entry.path().join(SCHEMA_FILE_NAME);
            let schema = Self::load_schema(&schema_file, &namespace, &namespace_validator)?;
            schemas.insert(namespace, schema);
        }

        Ok(schemas)
    }

    /// Load a schema from a file
    fn load_schema(
        path: &Path,
        namespace: &str,
        namespace_validator: &jsonschema::Validator,
    ) -> ValidationResult<Arc<NamespaceSchema>> {
        let file = fs::File::open(path)?;
        let schema_data: Value = serde_json::from_reader(file)?;

        Self::validate_with_namespace_schema(&schema_data, path, namespace_validator)?;
        Self::parse_schema(schema_data, namespace, path)
    }

    /// Validate a schema against the namespace-schema
    fn validate_with_namespace_schema(
        schema_data: &Value,
        path: &Path,
        namespace_validator: &jsonschema::Validator,
    ) -> ValidationResult<()> {
        let output = namespace_validator.evaluate(schema_data);

        if output.flag().valid {
            Ok(())
        } else {
            let errors: Vec<String> = output
                .iter_errors()
                .map(|e| format!("Error: {}", e.error))
                .collect();

            Err(ValidationError::SchemaError {
                file: path.to_path_buf(),
                message: format!("Schema validation failed:\n{}", errors.join("\n")),
            })
        }
    }

    /// Validate that a default value matches its declared type using jsonschema
    fn validate_default_type(
        property_name: &str,
        property_type: &str,
        default_value: &Value,
        path: &Path,
    ) -> ValidationResult<()> {
        // Build a mini JSON Schema for just this type
        let type_schema = serde_json::json!({
            "type": property_type
        });

        // Validate the default value against the type
        jsonschema::validate(&type_schema, default_value).map_err(|e| {
            ValidationError::SchemaError {
                file: path.to_path_buf(),
                message: format!(
                    "Property '{}': default value does not match type '{}': {}",
                    property_name, property_type, e
                ),
            }
        })?;

        Ok(())
    }

    /// Parse a schema JSON into NamespaceSchema
    fn parse_schema(
        mut schema: Value,
        namespace: &str,
        path: &Path,
    ) -> ValidationResult<Arc<NamespaceSchema>> {
        // Inject additionalProperties: false to reject unknown options
        if let Some(obj) = schema.as_object_mut() {
            obj.insert("additionalProperties".to_string(), json!(false));
        }

        // Use the schema file directly as the validator
        let validator =
            jsonschema::validator_for(&schema).map_err(|e| ValidationError::SchemaError {
                file: path.to_path_buf(),
                message: format!("Failed to compile validator: {}", e),
            })?;

        // Extract defaults and validate types
        let mut defaults = HashMap::new();
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            for (prop_name, prop_value) in properties {
                if let (Some(prop_type), Some(default_value)) = (
                    prop_value.get("type").and_then(|t| t.as_str()),
                    prop_value.get("default"),
                ) {
                    Self::validate_default_type(prop_name, prop_type, default_value, path)?;
                    defaults.insert(prop_name.clone(), default_value.clone());
                }
            }
        }

        Ok(Arc::new(NamespaceSchema {
            namespace: namespace.to_string(),
            defaults,
            validator,
        }))
    }

    /// Get a namespace schema by name
    pub fn get(&self, namespace: &str) -> Option<&Arc<NamespaceSchema>> {
        self.schemas.get(namespace)
    }

    /// Load and validate JSON values from a directory.
    /// Expects structure: `{values_dir}/{namespace}/values.json`
    /// Skips namespaces without a values.json file.
    pub fn load_values_json(&self, values_dir: &Path) -> ValidationResult<ValuesByNamespace> {
        let mut all_values = HashMap::new();

        for namespace in self.schemas.keys() {
            let values_file = values_dir.join(namespace).join(VALUES_FILE_NAME);

            if !values_file.exists() {
                continue;
            }

            let values: Value = serde_json::from_reader(fs::File::open(&values_file)?)?;
            self.validate_values(namespace, &values)?;

            if let Value::Object(obj) = values {
                let ns_values: HashMap<String, Value> = obj.into_iter().collect();
                all_values.insert(namespace.clone(), ns_values);
            }
        }

        Ok(all_values)
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Watches the values directory for changes, reloading if there are any.
/// If the directory does not exist we do not panic
///
/// Does not do an initial fetch, assumes the caller has already loaded values.
/// Child thread may panic if we run out of memory or cannot create more threads.
///
/// Uses polling for now, could use `inotify` or similar later on.
///
/// Some important notes:
/// - If the thread panics and dies, there is no built in mechanism to catch it and restart
/// - If a config map is unmounted, we won't reload until the next file modification (because we don't catch the deletion event)
/// - If any namespace fails validation, we keep all old values (even the namespaces that passed validation)
/// - If we have a steady stream of readers our writer may starve for a while trying to acquire the lock
/// - stop() will block until the thread gets joined
pub struct ValuesWatcher {
    stop_signal: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ValuesWatcher {
    /// Creates a new ValuesWatcher struct and spins up the watcher thread
    pub fn new(
        values_path: &Path,
        registry: Arc<SchemaRegistry>,
        values: Arc<RwLock<ValuesByNamespace>>,
    ) -> ValidationResult<Self> {
        // output an error but keep passing
        if fs::metadata(values_path).is_err() {
            eprintln!("Values directory does not exist: {}", values_path.display());
        }

        let stop_signal = Arc::new(AtomicBool::new(false));

        let thread_signal = Arc::clone(&stop_signal);
        let thread_path = values_path.to_path_buf();
        let thread_registry = Arc::clone(&registry);
        let thread_values = Arc::clone(&values);
        let thread = thread::Builder::new()
            .name("sentry-options-watcher".into())
            .spawn(move || {
                let result = panic::catch_unwind(AssertUnwindSafe(|| {
                    Self::run(thread_signal, thread_path, thread_registry, thread_values);
                }));
                if let Err(e) = result {
                    eprintln!("Watcher thread panicked with: {:?}", e);
                }
            })?;

        Ok(Self {
            stop_signal,
            thread: Some(thread),
        })
    }

    /// Reloads the values if the modified time has changed.
    ///
    /// Continuously polls the values directory and reloads all values
    /// if any modification is detected.
    fn run(
        stop_signal: Arc<AtomicBool>,
        values_path: PathBuf,
        registry: Arc<SchemaRegistry>,
        values: Arc<RwLock<ValuesByNamespace>>,
    ) {
        let mut last_mtime = Self::get_mtime(&values_path);

        while !stop_signal.load(Ordering::Relaxed) {
            // does not reload values if get_mtime fails
            if let Some(current_mtime) = Self::get_mtime(&values_path)
                && Some(current_mtime) != last_mtime
            {
                Self::reload_values(&values_path, &registry, &values);
                last_mtime = Some(current_mtime);
            }

            thread::sleep(Duration::from_secs(POLLING_DELAY));
        }
    }

    /// Get the most recent modification time across all namespace values.json files
    /// Returns None if no valid values files are found
    fn get_mtime(values_dir: &Path) -> Option<std::time::SystemTime> {
        let mut latest_mtime = None;

        let entries = match fs::read_dir(values_dir) {
            Ok(e) => e,
            Err(e) => {
                eprintln!("Failed to read directory {}: {}", values_dir.display(), e);
                return None;
            }
        };

        for entry in entries.flatten() {
            // skip if not a dir
            if !entry
                .file_type()
                .map(|file_type| file_type.is_dir())
                .unwrap_or(false)
            {
                continue;
            }

            let values_file = entry.path().join(VALUES_FILE_NAME);
            if let Ok(metadata) = fs::metadata(&values_file)
                && let Ok(mtime) = metadata.modified()
                && latest_mtime.is_none_or(|latest| mtime > latest)
            {
                latest_mtime = Some(mtime);
            }
        }

        latest_mtime
    }

    /// Reload values from disk, validate them, and update the shared map
    fn reload_values(
        values_path: &Path,
        registry: &SchemaRegistry,
        values: &Arc<RwLock<ValuesByNamespace>>,
    ) {
        match registry.load_values_json(values_path) {
            Ok(new_values) => {
                Self::update_values(values, new_values);
                // TODO: add some logging here so we know when options refresh
            }
            Err(e) => {
                eprintln!(
                    "Failed to reload values from {}: {}",
                    values_path.display(),
                    e
                );
            }
        }
    }

    /// Update the values map with the new values
    fn update_values(values: &Arc<RwLock<ValuesByNamespace>>, new_values: ValuesByNamespace) {
        // safe to unwrap, we only have one thread and if it panics we die anyways
        let mut guard = values.write().unwrap();
        *guard = new_values;
    }

    /// Stops the watcher thread, waiting for it to join.
    /// May take up to POLLING_DELAY seconds
    pub fn stop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }

    /// Returns whether the watcher thread is still running
    pub fn is_alive(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }
}

impl Drop for ValuesWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_schema(temp_dir: &TempDir, namespace: &str, schema_json: &str) -> PathBuf {
        let schema_dir = temp_dir.path().join(namespace);
        fs::create_dir_all(&schema_dir).unwrap();
        let schema_file = schema_dir.join("schema.json");
        fs::write(&schema_file, schema_json).unwrap();
        schema_file
    }

    #[test]
    fn test_load_schema_valid() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "test-key": {
                        "type": "string",
                        "default": "test",
                        "description": "Test option"
                    }
                }
            }"#,
        );

        SchemaRegistry::from_directory(temp_dir.path()).unwrap();
    }

    #[test]
    fn test_load_schema_missing_version() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "type": "object",
                "properties": {}
            }"#,
        );

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(message.contains(
                    "Schema validation failed:
Error: \"version\" is a required property"
                ));
            }
            _ => panic!("Expected SchemaError for missing version"),
        }
    }

    #[test]
    fn test_unknown_namespace() {
        let temp_dir = TempDir::new().unwrap();
        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        let result = registry.validate_values("unknown", &json!({}));
        assert!(matches!(result, Err(ValidationError::UnknownNamespace(..))));
    }

    #[test]
    fn test_multiple_namespaces() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "ns1",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "opt1": {
                        "type": "string",
                        "default": "default1",
                        "description": "First option"
                    }
                }
            }"#,
        );
        create_test_schema(
            &temp_dir,
            "ns2",
            r#"{
                "version": "2.0",
                "type": "object",
                "properties": {
                    "opt2": {
                        "type": "integer",
                        "default": 42,
                        "description": "Second option"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        assert!(registry.schemas.contains_key("ns1"));
        assert!(registry.schemas.contains_key("ns2"));
    }

    #[test]
    fn test_invalid_default_type() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "bad-default": {
                        "type": "integer",
                        "default": "not-a-number",
                        "description": "A bad default value"
                    }
                }
            }"#,
        );

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(message.contains("Property 'bad-default': default value does not match type 'integer': \"not-a-number\" is not of type \"integer\""));
            }
            _ => panic!("Expected SchemaError for invalid default type"),
        }
    }

    #[test]
    fn test_extra_properties() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "bad-property": {
                        "type": "integer",
                        "default": 0,
                        "description": "Test property",
                        "extra": "property"
                    }
                }
            }"#,
        );

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(
                    message
                        .contains("Additional properties are not allowed ('extra' was unexpected)")
                );
            }
            _ => panic!("Expected SchemaError for extra properties"),
        }
    }

    #[test]
    fn test_missing_description() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "missing-desc": {
                        "type": "string",
                        "default": "test"
                    }
                }
            }"#,
        );

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(message.contains("\"description\" is a required property"));
            }
            _ => panic!("Expected SchemaError for missing description"),
        }
    }

    #[test]
    fn test_invalid_directory_structure() {
        let temp_dir = TempDir::new().unwrap();
        // Create a namespace directory without schema.json file
        let schema_dir = temp_dir.path().join("missing_schema");
        fs::create_dir_all(&schema_dir).unwrap();

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::FileRead(..)) => {
                // Expected error when schema.json file is missing
            }
            _ => panic!("Expected FileRead error for missing schema.json"),
        }
    }

    #[test]
    fn test_get_default() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "string_opt": {
                        "type": "string",
                        "default": "hello",
                        "description": "A string option"
                    },
                    "int_opt": {
                        "type": "integer",
                        "default": 42,
                        "description": "An integer option"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        let schema = registry.get("test").unwrap();

        assert_eq!(schema.get_default("string_opt"), Some(&json!("hello")));
        assert_eq!(schema.get_default("int_opt"), Some(&json!(42)));
        assert_eq!(schema.get_default("unknown"), None);
    }

    #[test]
    fn test_validate_values_valid() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
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

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        let result = registry.validate_values("test", &json!({"enabled": true}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_values_invalid_type() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "count": {
                        "type": "integer",
                        "default": 0,
                        "description": "Count"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        let result = registry.validate_values("test", &json!({"count": "not a number"}));
        assert!(matches!(result, Err(ValidationError::ValueError { .. })));
    }

    #[test]
    fn test_validate_values_unknown_option() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "known_option": {
                        "type": "string",
                        "default": "default",
                        "description": "A known option"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        // Valid known option should pass
        let result = registry.validate_values("test", &json!({"known_option": "value"}));
        assert!(result.is_ok());

        // Unknown option should fail
        let result = registry.validate_values("test", &json!({"unknown_option": "value"}));
        assert!(result.is_err());
        match result {
            Err(ValidationError::ValueError { errors, .. }) => {
                assert!(errors.contains("Additional properties are not allowed"));
            }
            _ => panic!("Expected ValueError for unknown option"),
        }
    }

    #[test]
    fn test_load_values_json_valid() {
        let temp_dir = TempDir::new().unwrap();
        let schemas_dir = temp_dir.path().join("schemas");
        let values_dir = temp_dir.path().join("values");

        let schema_dir = schemas_dir.join("test");
        fs::create_dir_all(&schema_dir).unwrap();
        fs::write(
            schema_dir.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "enabled": {
                        "type": "boolean",
                        "default": false,
                        "description": "Enable feature"
                    },
                    "name": {
                        "type": "string",
                        "default": "default",
                        "description": "Name"
                    },
                    "count": {
                        "type": "integer",
                        "default": 0,
                        "description": "Count"
                    },
                    "rate": {
                        "type": "number",
                        "default": 0.0,
                        "description": "Rate"
                    }
                }
            }"#,
        )
        .unwrap();

        let test_values_dir = values_dir.join("test");
        fs::create_dir_all(&test_values_dir).unwrap();
        fs::write(
            test_values_dir.join("values.json"),
            r#"{
                "enabled": true,
                "name": "test-name",
                "count": 42,
                "rate": 0.75
            }"#,
        )
        .unwrap();

        let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
        let values = registry.load_values_json(&values_dir).unwrap();

        assert_eq!(values.len(), 1);
        assert_eq!(values["test"]["enabled"], json!(true));
        assert_eq!(values["test"]["name"], json!("test-name"));
        assert_eq!(values["test"]["count"], json!(42));
        assert_eq!(values["test"]["rate"], json!(0.75));
    }

    #[test]
    fn test_load_values_json_nonexistent_dir() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{"version": "1.0", "type": "object", "properties": {}}"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        let values = registry
            .load_values_json(&temp_dir.path().join("nonexistent"))
            .unwrap();

        // No values.json files found, returns empty
        assert!(values.is_empty());
    }

    #[test]
    fn test_load_values_json_skips_missing_values_file() {
        let temp_dir = TempDir::new().unwrap();
        let schemas_dir = temp_dir.path().join("schemas");
        let values_dir = temp_dir.path().join("values");

        // Create two schemas
        let schema_dir1 = schemas_dir.join("with_values");
        fs::create_dir_all(&schema_dir1).unwrap();
        fs::write(
            schema_dir1.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "opt": {"type": "string", "default": "x", "description": "Opt"}
                }
            }"#,
        )
        .unwrap();

        let schema_dir2 = schemas_dir.join("without_values");
        fs::create_dir_all(&schema_dir2).unwrap();
        fs::write(
            schema_dir2.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "opt": {"type": "string", "default": "x", "description": "Opt"}
                }
            }"#,
        )
        .unwrap();

        // Only create values for one namespace
        let with_values_dir = values_dir.join("with_values");
        fs::create_dir_all(&with_values_dir).unwrap();
        fs::write(with_values_dir.join("values.json"), r#"{"opt": "y"}"#).unwrap();

        let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
        let values = registry.load_values_json(&values_dir).unwrap();

        assert_eq!(values.len(), 1);
        assert!(values.contains_key("with_values"));
        assert!(!values.contains_key("without_values"));
    }

    #[test]
    fn test_load_values_json_rejects_wrong_type() {
        let temp_dir = TempDir::new().unwrap();
        let schemas_dir = temp_dir.path().join("schemas");
        let values_dir = temp_dir.path().join("values");

        let schema_dir = schemas_dir.join("test");
        fs::create_dir_all(&schema_dir).unwrap();
        fs::write(
            schema_dir.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "count": {"type": "integer", "default": 0, "description": "Count"}
                }
            }"#,
        )
        .unwrap();

        let test_values_dir = values_dir.join("test");
        fs::create_dir_all(&test_values_dir).unwrap();
        fs::write(
            test_values_dir.join("values.json"),
            r#"{"count": "not-a-number"}"#,
        )
        .unwrap();

        let registry = SchemaRegistry::from_directory(&schemas_dir).unwrap();
        let result = registry.load_values_json(&values_dir);

        assert!(matches!(result, Err(ValidationError::ValueError { .. })));
    }

    mod watcher_tests {
        use super::*;
        use std::thread;

        /// Creates schema and values files for two namespaces: ns1, and ns2
        fn setup_watcher_test() -> (TempDir, PathBuf, PathBuf) {
            let temp_dir = TempDir::new().unwrap();
            let schemas_dir = temp_dir.path().join("schemas");
            let values_dir = temp_dir.path().join("values");

            let ns1_schema = schemas_dir.join("ns1");
            fs::create_dir_all(&ns1_schema).unwrap();
            fs::write(
                ns1_schema.join("schema.json"),
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "enabled": {"type": "boolean", "default": false, "description": "Enabled"}
                    }
                }"#,
            )
            .unwrap();

            let ns1_values = values_dir.join("ns1");
            fs::create_dir_all(&ns1_values).unwrap();
            fs::write(ns1_values.join("values.json"), r#"{"enabled": true}"#).unwrap();

            let ns2_schema = schemas_dir.join("ns2");
            fs::create_dir_all(&ns2_schema).unwrap();
            fs::write(
                ns2_schema.join("schema.json"),
                r#"{
                    "version": "1.0",
                    "type": "object",
                    "properties": {
                        "count": {"type": "integer", "default": 0, "description": "Count"}
                    }
                }"#,
            )
            .unwrap();

            let ns2_values = values_dir.join("ns2");
            fs::create_dir_all(&ns2_values).unwrap();
            fs::write(ns2_values.join("values.json"), r#"{"count": 42}"#).unwrap();

            (temp_dir, schemas_dir, values_dir)
        }

        #[test]
        fn test_get_mtime_returns_most_recent() {
            let (_temp, _schemas, values_dir) = setup_watcher_test();

            // Get initial mtime
            let mtime1 = ValuesWatcher::get_mtime(&values_dir);
            assert!(mtime1.is_some());

            // Modify one namespace
            thread::sleep(std::time::Duration::from_millis(10));
            fs::write(
                values_dir.join("ns1").join("values.json"),
                r#"{"enabled": false}"#,
            )
            .unwrap();

            // Should detect the change
            let mtime2 = ValuesWatcher::get_mtime(&values_dir);
            assert!(mtime2.is_some());
            assert!(mtime2 > mtime1);
        }

        #[test]
        fn test_get_mtime_with_missing_directory() {
            let temp = TempDir::new().unwrap();
            let nonexistent = temp.path().join("nonexistent");

            let mtime = ValuesWatcher::get_mtime(&nonexistent);
            assert!(mtime.is_none());
        }

        #[test]
        fn test_reload_values_updates_map() {
            let (_temp, schemas_dir, values_dir) = setup_watcher_test();

            let registry = Arc::new(SchemaRegistry::from_directory(&schemas_dir).unwrap());
            let initial_values = registry.load_values_json(&values_dir).unwrap();
            let values = Arc::new(RwLock::new(initial_values));

            // ensure initial values are correct
            {
                let guard = values.read().unwrap();
                assert_eq!(guard["ns1"]["enabled"], json!(true));
                assert_eq!(guard["ns2"]["count"], json!(42));
            }

            // modify
            fs::write(
                values_dir.join("ns1").join("values.json"),
                r#"{"enabled": false}"#,
            )
            .unwrap();
            fs::write(
                values_dir.join("ns2").join("values.json"),
                r#"{"count": 100}"#,
            )
            .unwrap();

            // force a reload
            ValuesWatcher::reload_values(&values_dir, &registry, &values);

            // ensure new values are correct
            {
                let guard = values.read().unwrap();
                assert_eq!(guard["ns1"]["enabled"], json!(false));
                assert_eq!(guard["ns2"]["count"], json!(100));
            }
        }

        #[test]
        fn test_old_values_persist_with_invalid_data() {
            let (_temp, schemas_dir, values_dir) = setup_watcher_test();

            let registry = Arc::new(SchemaRegistry::from_directory(&schemas_dir).unwrap());
            let initial_values = registry.load_values_json(&values_dir).unwrap();
            let values = Arc::new(RwLock::new(initial_values));

            let initial_enabled = {
                let guard = values.read().unwrap();
                guard["ns1"]["enabled"].clone()
            };

            // won't pass validation
            fs::write(
                values_dir.join("ns1").join("values.json"),
                r#"{"enabled": "not-a-boolean"}"#,
            )
            .unwrap();

            ValuesWatcher::reload_values(&values_dir, &registry, &values);

            // ensure old value persists
            {
                let guard = values.read().unwrap();
                assert_eq!(guard["ns1"]["enabled"], initial_enabled);
            }
        }

        #[test]
        fn test_watcher_creation_and_termination() {
            let (_temp, schemas_dir, values_dir) = setup_watcher_test();

            let registry = Arc::new(SchemaRegistry::from_directory(&schemas_dir).unwrap());
            let initial_values = registry.load_values_json(&values_dir).unwrap();
            let values = Arc::new(RwLock::new(initial_values));

            let mut watcher =
                ValuesWatcher::new(&values_dir, Arc::clone(&registry), Arc::clone(&values))
                    .expect("Failed to create watcher");

            assert!(watcher.is_alive());
            watcher.stop();
            assert!(!watcher.is_alive());
        }
    }
}
