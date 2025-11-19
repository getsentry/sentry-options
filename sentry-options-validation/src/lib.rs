//! Schema validation library for sentry-options
//!
//! This library provides schema loading and validation for sentry-options.
//! Schemas are loaded once and stored in Arc for efficient sharing.
//! Values are validated against schemas as complete objects.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

/// Embedded meta-schema for validating sentry-options schema files
const NAMESPACE_SCHEMA_JSON: &str = include_str!("namespace-schema.json");

/// Result type for validation operations
pub type ValidationResult<T> = Result<T, ValidationError>;

/// Errors that can occur during schema and value validation
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Schema error in {file}: {message}")]
    SchemaError { file: PathBuf, message: String },

    #[error("Value error for {namespace}: {errors}")]
    ValueError { namespace: String, errors: String },

    #[error("Unknown namespace: {0}")]
    UnknownNamespace(String),

    #[error("Failed to read file: {0}")]
    FileRead(#[from] std::io::Error),

    #[error("Failed to parse JSON: {0}")]
    JSONParse(#[from] serde_json::Error),
}

/// Schema for a namespace, containing only a validator
pub struct NamespaceSchema {
    pub namespace: String,
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

        if output.iter_errors().count() == 0 {
            Ok(())
        } else {
            let errors: Vec<String> = output
                .iter_errors()
                .map(|e| format!("Error: {}", e.error))
                .collect();

            Err(ValidationError::ValueError {
                namespace: self.namespace.clone(),
                errors: errors.join("\n"),
            })
        }
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

    /// Get a namespace schema by name
    pub fn get_schema(&self, namespace: &str) -> Option<Arc<NamespaceSchema>> {
        self.schemas.get(namespace).cloned()
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
        if !schemas_dir.exists() {
            return Err(ValidationError::FileRead(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Schemas directory not found: {}", schemas_dir.display()),
            )));
        }

        // Compile namespace-schema once for all schemas
        let namespace_schema_value: Value =
            serde_json::from_str(NAMESPACE_SCHEMA_JSON).expect("Invalid namespace-schema JSON");
        let namespace_validator = jsonschema::validator_for(&namespace_schema_value)
            .expect("Failed to compile namespace-schema");

        let mut schemas = HashMap::new();

        // TODO: Parallelize the loading of schemas for the performance gainz
        for entry in fs::read_dir(schemas_dir)? {
            let entry = entry?;

            if !entry.file_type()?.is_dir() {
                continue;
            }

            let namespace = entry
                .file_name().into_string().unwrap();

            let schema_file = entry.path().join("schema.json");

            if schema_file.exists() {
                let schema = Self::load_schema(&schema_file, &namespace, &namespace_validator)?;
                schemas.insert(namespace, schema);
            }
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

        if output.iter_errors().count() == 0 {
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
        schema: Value,
        namespace: &str,
        path: &Path,
    ) -> ValidationResult<Arc<NamespaceSchema>> {
        // Validate that default values match their types
        if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
            for (prop_name, prop_value) in properties {
                if let (Some(prop_type), Some(default_value)) = (
                    prop_value.get("type").and_then(|t| t.as_str()),
                    prop_value.get("default"),
                ) {
                    Self::validate_default_type(prop_name, prop_type, default_value, path)?;
                }
            }
        }

        // Use the schema file directly as the validator
        let validator = jsonschema::validator_for(&schema).map_err(|e| {
            ValidationError::SchemaError {
                file: path.to_path_buf(),
                message: format!("Failed to compile validator: {}", e),
            }
        })?;

        Ok(Arc::new(NamespaceSchema {
            namespace: namespace.to_string(),
            validator,
        }))
    }
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
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

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        let schema = registry.get_schema("test").unwrap();
        assert_eq!(schema.namespace, "test");
    }

    #[test]
    fn test_validate_values_success() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "string-opt": {
                        "type": "string",
                        "default": "default"
                    },
                    "int-opt": {
                        "type": "integer",
                        "default": 42
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        let values = json!({
            "string-opt": "hello",
            "int-opt": 123
        });

        assert!(registry.validate_values("test", &values).is_ok());
    }

    #[test]
    fn test_validate_values_type_mismatch() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "string-opt": {
                        "type": "string",
                        "default": "default"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        let values = json!({
            "string-opt": 42
        });

        let result = registry.validate_values("test", &values);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ValidationError::ValueError { .. }));
    }

    #[test]
    fn test_validate_values_additional_properties() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "known-opt": {
                        "type": "string",
                        "default": "default"
                    }
                },
                "additionalProperties": false
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        let values = json!({
            "known-opt": "value",
            "unknown-opt": "should-fail"
        });

        let result = registry.validate_values("test", &values);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_values_partial() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "opt1": {
                        "type": "string",
                        "default": "default1"
                    },
                    "opt2": {
                        "type": "integer",
                        "default": 42
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        // Partial values should be OK (values not present will use defaults)
        let values = json!({
            "opt1": "custom-value"
        });

        assert!(registry.validate_values("test", &values).is_ok());
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
        assert!(matches!(result, Err(ValidationError::SchemaError { .. })));
    }

    #[test]
    fn test_unknown_namespace() {
        let temp_dir = TempDir::new().unwrap();
        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        let result = registry.validate_values("unknown", &json!({}));
        assert!(matches!(
            result,
            Err(ValidationError::UnknownNamespace(_))
        ));
    }

    #[test]
    fn test_all_primitive_types() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "str-opt": {
                        "type": "string",
                        "default": ""
                    },
                    "int-opt": {
                        "type": "integer",
                        "default": 0
                    },
                    "num-opt": {
                        "type": "number",
                        "default": 0.0
                    },
                    "bool-opt": {
                        "type": "boolean",
                        "default": false
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        let values = json!({
            "str-opt": "hello",
            "int-opt": 42,
            "num-opt": 3.14,
            "bool-opt": true
        });

        assert!(registry.validate_values("test", &values).is_ok());
    }

    #[test]
    fn test_empty_values_object() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "opt1": {
                        "type": "string",
                        "default": "default"
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        // Empty values object should be valid (all options will use defaults)
        let values = json!({});
        assert!(registry.validate_values("test", &values).is_ok());
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
                        "default": "default1"
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
                        "default": 42
                    }
                }
            }"#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        assert!(registry.get_schema("ns1").is_some());
        assert!(registry.get_schema("ns2").is_some());
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
                        "default": "not-a-number"
                    }
                }
            }"#,
        );

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(message.contains("default value"));
                assert!(message.contains("does not match type"));
            }
            _ => panic!("Expected SchemaError"),
        }
    }
}
