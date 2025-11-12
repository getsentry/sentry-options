//! Schema validation library for sentry-options
//!
//! This library provides runtime validation of option schemas and values.
//!

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use thiserror::Error;

use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::PathBuf;

/// Result type for validation operations
pub type ValidationResult<T> = std::result::Result<T, ValidationError>;

/// Errors that can occur during schema and value validation
#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("Schema error in {file}: {message}")]
    SchemaError { file: PathBuf, message: String },

    #[error("Value error for {namespace}.{key}: {message}")]
    ValueError {
        namespace: String,
        key: String,
        message: String,
    },

    #[error("Type mismatch for {namespace}.{key}: expected {expected}, got {actual}")]
    TypeMismatch {
        namespace: String,
        key: String,
        expected: String,
        actual: String,
    },

    #[error("Unknown namespace: {0}")]
    UnknownNamespace(String),

    #[error("Unsupported type: {0}")]
    UnsupportedType(String),

    #[error("Failed to read file: {0}")]
    FileRead(#[from] std::io::Error),

    #[error("Failed to parse JSON: {0}")]
    JSONParse(#[from] serde_json::Error),
}

// Error construction helpers
impl ValidationError {
    fn schema_error(file: impl AsRef<Path>, message: impl Into<String>) -> Self {
        Self::SchemaError {
            file: file.as_ref().to_path_buf(),
            message: message.into(),
        }
    }

    fn value_error(namespace: impl Into<String>, key: impl Into<String>, message: impl Into<String>) -> Self {
        Self::ValueError {
            namespace: namespace.into(),
            key: key.into(),
            message: message.into(),
        }
    }

    fn type_mismatch(
        namespace: impl Into<String>,
        key: impl Into<String>,
        expected: impl Into<String>,
        actual: impl Into<String>,
    ) -> Self {
        Self::TypeMismatch {
            namespace: namespace.into(),
            key: key.into(),
            expected: expected.into(),
            actual: actual.into(),
        }
    }
}

/// Represents a complete schema for a namespace
#[derive(Deserialize)]
pub struct NamespaceSchema {
    pub version: String,
    #[serde(deserialize_with = "deserialize_options")]
    pub options: HashMap<String, Arc<OptionSchema>>,
}

/// Custom deserializer to wrap OptionSchemas in Arc
fn deserialize_options<'de, D>(deserializer: D) -> Result<HashMap<String, Arc<OptionSchema>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let map: HashMap<String, OptionSchema> = HashMap::deserialize(deserializer)?;
    Ok(map
        .into_iter()
        .map(|(k, v)| (k, Arc::new(v)))
        .collect::<HashMap<String, Arc<OptionSchema>>>())
}

/// Represents a single option's schema definition
#[derive(Deserialize)]
pub struct OptionSchema {
    pub option_type: String,
    pub default: Value,
    pub description: Option<String>,
}

/// Schema type enumeration
#[derive(Clone, Copy, PartialEq, Eq)]
enum OptionType {
    Str,
    Int,
    Float,
    Bool,
}

impl OptionType {
    /// Parse schema type from string
    ///
    /// # Arguments
    /// * `s` - Schema type string ("str", "int", "float", "bool")
    ///
    /// # Errors
    /// Returns UnsupportedType if the string is not a valid schema type
    fn from_str(s: &str) -> ValidationResult<Self> {
        match s {
            "str" => Ok(Self::Str),
            "int" => Ok(Self::Int),
            "float" => Ok(Self::Float),
            "bool" => Ok(Self::Bool),
            _ => Err(ValidationError::UnsupportedType(s.to_string())),
        }
    }

    /// Get the Rust type name for error messages
    fn to_rust_type(self) -> &'static str {
        match self {
            Self::Str => "String",
            Self::Int => "i64",
            Self::Float => "f64",
            Self::Bool => "bool",
        }
    }

    /// Check if a value matches this schema type
    fn is_valid(self, value: &Value) -> bool {
        match self {
            Self::Str => value.is_string(),
            Self::Int => value.is_i64(),
            Self::Float => value.is_f64(),
            Self::Bool => value.is_boolean(),
        }
    }
}

impl OptionSchema {
    /// Validate a value against this option schema
    ///
    /// # Arguments
    /// * `namespace` - Namespace name (for error messages)
    /// * `key` - Option key (for error messages)
    /// * `value` - JSON value to validate
    ///
    /// # Errors
    /// Returns error if value doesn't match the schema type
    pub fn validate(&self, namespace: &str, key: &str, value: &Value) -> ValidationResult<()> {
        // Parse and validate schema type
        let schema_type = OptionType::from_str(&self.option_type)
            .map_err(|_| ValidationError::value_error(
                namespace,
                key,
                format!("Unknown type: {}", self.option_type),
            ))?;

        // Check if value matches the schema type
        if !schema_type.is_valid(value) {
            return Err(ValidationError::type_mismatch(
                namespace,
                key,
                schema_type.to_rust_type(),
                value_type_name(value),
            ));
        }

        Ok(())
    }
}

/// Schema registry for loading, storing, and validating schemas
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
    ///
    /// # Arguments
    /// * `namespace` - The namespace to look up
    ///
    /// # Returns
    /// Some(Arc<NamespaceSchema>) if namespace exists, None otherwise
    pub fn get_schema(&self, namespace: &str) -> Option<Arc<NamespaceSchema>> {
        self.schemas.get(namespace).cloned()
    }

    /// Get a specific option schema
    ///
    /// # Arguments
    /// * `namespace` - The namespace
    /// * `key` - The option key
    ///
    /// # Returns
    /// Some(Arc<OptionSchema>) if found, None otherwise
    pub fn get_option_schema(&self, namespace: &str, key: &str) -> Option<Arc<OptionSchema>> {
        self.schemas
            .get(namespace)?
            .options
            .get(key)
            .cloned()
    }

    /// Load a schema from a file path
    ///
    /// # Arguments
    /// * `path` - Path to schema.json file
    ///
    /// # Errors
    /// Returns error if file doesn't exist, is invalid JSON, or fails validation
    fn load_schema(path: &Path) -> ValidationResult<Arc<NamespaceSchema>> {
        let content = fs::read_to_string(path)?;
        let schema_value: Value = serde_json::from_str(&content)?;

        Self::validate_schema_structure(&schema_value, path)?;

        let schema: NamespaceSchema = serde_json::from_value(schema_value).map_err(|e| {
            ValidationError::schema_error(path, format!("Failed to parse schema: {}", e))
        })?;

        Ok(Arc::new(schema))
    }

    /// Load all schemas from a directory
    ///
    /// Expects directory structure: `schemas/{namespace}/schema.json`
    ///
    /// # Arguments
    /// * `schemas_dir` - Path to directory containing namespace subdirectories
    ///
    /// # Errors
    /// Returns error if directory doesn't exist or any schema is invalid
    fn load_all_schemas(schemas_dir: &Path) -> ValidationResult<HashMap<String, Arc<NamespaceSchema>>> {
        if !schemas_dir.exists() {
            return Err(ValidationError::FileRead(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Schemas directory not found: {}", schemas_dir.display()),
            )));
        }

        let mut schemas = HashMap::new();

        for entry in fs::read_dir(schemas_dir)? {
            let entry = entry?;

            if !entry.file_type()?.is_dir() {
                continue;
            }

            let namespace = entry.file_name()
                .to_str()
                .ok_or_else(|| ValidationError::schema_error(
                    &entry.path(),
                    "Directory name contains invalid UTF-8"
                ))?
                .to_string();
            let schema_file = entry.path().join("schema.json");

            if schema_file.exists() {
                let schema = Self::load_schema(&schema_file)?;
                schemas.insert(namespace, schema);
            }
        }

        Ok(schemas)
    }

    /// Validate schema structure without deserializing
    ///
    /// Checks for required fields and basic structure
    fn validate_schema_structure(schema: &Value, path: &Path) -> ValidationResult<()> {
        let file = path.display().to_string();

        let obj = schema
            .as_object()
            .ok_or_else(|| ValidationError::schema_error(&file, "Schema must be a JSON object"))?;

        Self::validate_has_version(obj, &file)?;
        Self::validate_has_options(obj, &file)?;

        Ok(())
    }

    fn validate_has_version(obj: &Map<String, Value>, file: &str) -> ValidationResult<()> {
        if !obj.contains_key("version") {
            return Err(ValidationError::schema_error(file, "Missing required 'version' field"));
        }
        Ok(())
    }

    fn validate_has_options(obj: &Map<String, Value>, file: &str) -> ValidationResult<()> {
        let options = obj
            .get("options")
            .ok_or_else(|| ValidationError::schema_error(file, "Missing required 'options' field"))?;

        let options_obj = options
            .as_object()
            .ok_or_else(|| ValidationError::schema_error(file, "'options' field must be a JSON object"))?;

        for (key, option_value) in options_obj {
            Self::validate_option_definition(key, option_value, file)?;
        }

        Ok(())
    }

    fn validate_option_definition(key: &str, option: &Value, file: &str) -> ValidationResult<()> {
        let option_obj = option
            .as_object()
            .ok_or_else(|| ValidationError::schema_error(file, format!("Option '{}' must be a JSON object", key)))?;

        // Validate option_type field
        let type_value = option_obj
            .get("option_type")
            .ok_or_else(|| ValidationError::schema_error(file, format!("Option '{}' missing required 'option_type' field", key)))?;

        let type_str = type_value
            .as_str()
            .ok_or_else(|| ValidationError::schema_error(file, format!("Option '{}' 'option_type' field must be a string", key)))?;

        // Validate type is supported
        if !["str", "int", "float", "bool"].contains(&type_str) {
            return Err(ValidationError::schema_error(
                file,
                format!("Option '{}' has unsupported type: {}", key, type_str),
            ));
        }

        // Validate default field exists
        if !option_obj.contains_key("default") {
            return Err(ValidationError::schema_error(
                file,
                format!("Option '{}' missing required 'default' field", key),
            ));
        }

        Ok(())
    }

    /// Validate a single value against its schema
    ///
    /// # Arguments
    /// * `namespace` - Namespace name
    /// * `key` - Option key
    /// * `value` - JSON value to validate
    ///
    /// # Errors
    /// Returns error if namespace doesn't exist, key doesn't exist, or value doesn't match schema
    pub fn validate_value(&self, namespace: &str, key: &str, value: &Value) -> ValidationResult<()> {
        let schema = self.schemas.get(namespace)
            .ok_or_else(|| ValidationError::UnknownNamespace(namespace.to_string()))?;

        let option_schema = schema.options.get(key).ok_or_else(|| {
            ValidationError::value_error(namespace, key, "Unknown option (not defined in schema)")
        })?;

        option_schema.validate(namespace, key, value)
    }
}

/// Get a human-readable type name for a JSON value
fn value_type_name(value: &Value) -> String {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_i64() => "int",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn create_test_schema(temp_dir: &TempDir, namespace: &str, schema_json: &str) -> std::path::PathBuf {
        let schema_dir = temp_dir.path().join(namespace);
        fs::create_dir_all(&schema_dir).unwrap();
        let schema_file = schema_dir.join("schema.json");
        fs::write(&schema_file, schema_json).unwrap();
        schema_file
    }

    #[test]
    fn test_load_schema_valid() {
        let temp_dir = TempDir::new().unwrap();
        let _schema_file = create_test_schema(
            &temp_dir,
            "test",
            r#"
        {
            "version": "1.0",
            "options": {
                "test-key": {
                    "option_type": "str",
                    "default": "test",
                    "description": "Test option"
                }
            }
        }
        "#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        let schema = registry.get_schema("test").unwrap();
        assert_eq!(schema.version, "1.0");
        assert_eq!(schema.options.len(), 1);
        assert_eq!(schema.options["test-key"].option_type, "str");
    }

    #[test]
    fn test_load_schema_missing_version() {
        let temp_dir = TempDir::new().unwrap();
        let _schema_file = create_test_schema(
            &temp_dir,
            "test",
            r#"
        {
            "options": {}
        }
        "#,
        );

        let result = SchemaRegistry::from_directory(temp_dir.path());
        assert!(matches!(result, Err(ValidationError::SchemaError { .. })));
    }

    #[test]
    fn test_validate_value_string() {
        let option_schema = OptionSchema {
            option_type: "str".to_string(),
            default: json!("default"),
            description: None,
        };

        assert!(option_schema.validate("test", "key", &json!("value")).is_ok());

        let result = option_schema.validate("test", "key", &json!(42));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ValidationError::TypeMismatch { .. }));
    }

    #[test]
    fn test_validate_value_int() {
        let option_schema = OptionSchema {
            option_type: "int".to_string(),
            default: json!(0),
            description: None,
        };

        assert!(option_schema.validate("test", "key", &json!(42)).is_ok());

        let result = option_schema.validate("test", "key", &json!("not-an-int"));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_value_float() {
        let option_schema = OptionSchema {
            option_type: "float".to_string(),
            default: json!(0.0),
            description: None,
        };

        assert!(option_schema.validate("test", "key", &json!(3.14)).is_ok());

        let result = option_schema.validate("test", "key", &json!("not-a-float"));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_value_bool() {
        let option_schema = OptionSchema {
            option_type: "bool".to_string(),
            default: json!(false),
            description: None,
        };

        assert!(option_schema.validate("test", "key", &json!(true)).is_ok());

        let result = option_schema.validate("test", "key", &json!(1));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_all_schemas() {
        let temp_dir = TempDir::new().unwrap();

        create_test_schema(
            &temp_dir,
            "namespace1",
            r#"
        {
            "version": "1.0",
            "options": {}
        }
        "#,
        );

        create_test_schema(
            &temp_dir,
            "namespace2",
            r#"
        {
            "version": "1.0",
            "options": {}
        }
        "#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();
        assert!(registry.get_schema("namespace1").is_some());
        assert!(registry.get_schema("namespace2").is_some());
    }

    #[test]
    fn test_schema_registry_validate_value() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"
        {
            "version": "1.0",
            "options": {
                "string-opt": {
                    "option_type": "str",
                    "default": "default"
                },
                "int-opt": {
                    "option_type": "int",
                    "default": 42
                }
            }
        }
        "#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        // Valid values
        assert!(registry.validate_value("test", "string-opt", &json!("hello")).is_ok());
        assert!(registry.validate_value("test", "int-opt", &json!(123)).is_ok());

        // Type mismatches
        assert!(registry.validate_value("test", "string-opt", &json!(42)).is_err());
        assert!(registry.validate_value("test", "int-opt", &json!("not-an-int")).is_err());

        // Unknown namespace
        let result = registry.validate_value("unknown", "key", &json!("value"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ValidationError::UnknownNamespace(_)));

        // Unknown key
        let result = registry.validate_value("test", "unknown-key", &json!("value"));
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ValidationError::ValueError { .. }));
    }

    #[test]
    fn test_schema_registry_get_schema() {
        let temp_dir = TempDir::new().unwrap();
        create_test_schema(
            &temp_dir,
            "test",
            r#"
        {
            "version": "1.0",
            "options": {}
        }
        "#,
        );

        let registry = SchemaRegistry::from_directory(temp_dir.path()).unwrap();

        // Existing namespace
        assert!(registry.get_schema("test").is_some());

        // Non-existing namespace
        assert!(registry.get_schema("unknown").is_none());
    }
}
