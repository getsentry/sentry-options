use sentry_options_validation::{SchemaRegistry, ValidationError, ValidationResult};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const SCHEMA_FILE_NAME: &str = "schema.json";

/// Helper to extract properties from a schema with error handling.
fn get_all_options(
    schema: &Value,
    schema_file: &Path,
) -> ValidationResult<serde_json::Map<String, Value>> {
    schema
        .get("properties")
        .and_then(|p| p.as_object())
        .cloned()
        .ok_or_else(|| ValidationError::SchemaError {
            file: schema_file.to_path_buf(),
            message: "Schema missing 'properties' field".to_string(),
        })
}

/// Gets the type and default value for an option.
fn extract_option_meta(
    property: &Value,
    property_name: &str,
    file: &Path,
) -> ValidationResult<(String, Value)> {
    let prop_type = property
        .get("type")
        .and_then(|t| t.as_str())
        .ok_or_else(|| ValidationError::SchemaError {
            file: file.to_path_buf(),
            message: format!("Property '{}' missing 'type' field", property_name),
        })?
        .to_string();

    let default = property
        .get("default")
        .ok_or_else(|| ValidationError::SchemaError {
            file: file.to_path_buf(),
            message: format!("Property '{}' missing 'default' field", property_name),
        })?
        .clone();

    Ok((prop_type, default))
}

/// Load raw schema JSON files from a directory and maps namespace -> schema JSON
fn load_raw_schemas(schemas_dir: &Path) -> ValidationResult<HashMap<String, Value>> {
    let mut schemas = HashMap::new();

    for (namespace, schema_file) in SchemaRegistry::iter_namespace_dirs(schemas_dir)? {
        let schema_json: Value = serde_json::from_reader(fs::File::open(&schema_file)?)?;
        schemas.insert(namespace, schema_json);
    }

    Ok(schemas)
}

/// Compare two schemas and validate no breaking changes occurred
fn compare_schemas(
    namespace: &str,
    old_schema: &Value,
    new_schema: &Value,
    old_dir: &Path,
    new_dir: &Path,
) -> ValidationResult<()> {
    let old_file = old_dir.join(namespace).join(SCHEMA_FILE_NAME);
    let new_file = new_dir.join(namespace).join(SCHEMA_FILE_NAME);

    let old_props = get_all_options(old_schema, &old_file)?;
    let new_props = get_all_options(new_schema, &new_file)?;

    for (key, old_prop) in &old_props {
        // Skip if property was removed (allowed for now)
        let Some(new_prop) = new_props.get(key) else {
            continue;
        };

        // Extract type and default from both old and new properties
        let (old_type, old_default) = extract_option_meta(old_prop, key, &old_file)?;
        let (new_type, new_default) = extract_option_meta(new_prop, key, &new_file)?;

        // Check if type changed
        if old_type != new_type {
            return Err(ValidationError::SchemaError {
                file: new_file.clone(),
                message: format!(
                    "Property '{}.{}' type changed from '{}' to '{}'",
                    namespace, key, old_type, new_type
                ),
            });
        }

        // Check if default value changed
        if old_default != new_default {
            return Err(ValidationError::SchemaError {
                file: new_file.clone(),
                message: format!(
                    "Property '{}.{}' default value changed from {} to {}",
                    namespace, key, old_default, new_default
                ),
            });
        }
    }

    Ok(())
}

/// Compares 2 schema folders, representing old and new versions.
/// Assumes the schemas themselves are already validated.
/// Validates that any changes made are allowed, otherwise returns an error.
///
/// # Allowed changes:
/// - Adding new namespaces
/// - Adding new properties
///
/// # Allowed for now:
/// - Removing properties
///
/// # Breaking changes (will error):
/// - Removing namespaces
/// - Changing property types
/// - Changing default values
fn detect_changes(old_dir: &Path, new_dir: &Path) -> ValidationResult<()> {
    // Then load raw JSON for comparison
    let old_schemas = load_raw_schemas(old_dir)?;
    let new_schemas = load_raw_schemas(new_dir)?;

    // Check each old namespace exists in new schemas
    for (namespace, old_schema) in &old_schemas {
        let new_schema =
            new_schemas
                .get(namespace)
                .ok_or_else(|| ValidationError::SchemaError {
                    file: old_dir.join(namespace).join(SCHEMA_FILE_NAME),
                    message: format!("Namespace '{}' was removed", namespace),
                })?;

        compare_schemas(namespace, old_schema, new_schema, old_dir, new_dir)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    /// Helper to set up test directories
    fn setup_dirs() -> (TempDir, TempDir) {
        (TempDir::new().unwrap(), TempDir::new().unwrap())
    }

    /// Helper to build a property definition
    fn property(prop_type: &str, default: Value) -> Value {
        json!({
            "type": prop_type,
            "default": default,
            "description": "Test property"
        })
    }

    /// Helper to build a schema with given properties
    fn build_schema(properties: serde_json::Map<String, Value>) -> Value {
        json!({
            "version": "1.0",
            "type": "object",
            "properties": properties
        })
    }

    /// Helper to create a schema directory with a schema
    fn create_schema(temp_dir: &TempDir, namespace: &str, schema: &Value) {
        let schema_dir = temp_dir.path().join(namespace);
        fs::create_dir_all(&schema_dir).unwrap();
        let schema_file = schema_dir.join("schema.json");
        fs::write(&schema_file, serde_json::to_string_pretty(schema).unwrap()).unwrap();
    }

    /// Helper to modify a schema by cloning and applying changes to properties
    fn modify_schema(schema: &Value, f: impl FnOnce(&mut serde_json::Map<String, Value>)) -> Value {
        let mut new_schema = schema.clone();
        if let Some(props) = new_schema
            .get_mut("properties")
            .and_then(|p| p.as_object_mut())
        {
            f(props);
        }
        new_schema
    }

    #[test]
    fn test_identical_schemas_pass() {
        let (old_dir, new_dir) = setup_dirs();

        let mut props = serde_json::Map::new();
        props.insert("key1".to_string(), property("string", json!("test")));
        let schema = build_schema(props);

        create_schema(&old_dir, "test", &schema);
        create_schema(&new_dir, "test", &schema);

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_removed_namespace_fails() {
        let (old_dir, new_dir) = setup_dirs();

        let mut props = serde_json::Map::new();
        props.insert("key1".to_string(), property("string", json!("test")));
        let schema = build_schema(props);

        create_schema(&old_dir, "test", &schema);
        // new_dir is empty - namespace was removed

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(message.contains("Namespace 'test' was removed"));
            }
            _ => panic!("Expected SchemaError for removed namespace"),
        }
    }

    #[test]
    fn test_removed_property_passes() {
        let (old_dir, new_dir) = setup_dirs();

        // Build old schema with two properties
        let mut old_props = serde_json::Map::new();
        old_props.insert("key1".to_string(), property("string", json!("test")));
        old_props.insert("key2".to_string(), property("integer", json!(42)));
        let old_schema = build_schema(old_props);

        // Modify schema - remove key2
        let new_schema = modify_schema(&old_schema, |props| {
            props.remove("key2");
        });

        create_schema(&old_dir, "test", &old_schema);
        create_schema(&new_dir, "test", &new_schema);

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_type_change_fails() {
        let (old_dir, new_dir) = setup_dirs();

        // Build old schema
        let mut props = serde_json::Map::new();
        props.insert("key1".to_string(), property("string", json!("test")));
        let old_schema = build_schema(props);

        // Modify schema - change type
        let new_schema = modify_schema(&old_schema, |props| {
            props.insert("key1".to_string(), property("integer", json!(42)));
        });

        create_schema(&old_dir, "test", &old_schema);
        create_schema(&new_dir, "test", &new_schema);

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(message.contains("type changed from 'string' to 'integer'"));
            }
            _ => panic!("Expected SchemaError for type change"),
        }
    }

    #[test]
    fn test_default_value_change_fails() {
        let (old_dir, new_dir) = setup_dirs();

        // Build old schema
        let mut props = serde_json::Map::new();
        props.insert("key1".to_string(), property("string", json!("old-value")));
        let old_schema = build_schema(props);

        // Modify schema - change default value
        let new_schema = modify_schema(&old_schema, |props| {
            props.insert("key1".to_string(), property("string", json!("new-value")));
        });

        create_schema(&old_dir, "test", &old_schema);
        create_schema(&new_dir, "test", &new_schema);

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(message.contains("default value changed"));
                assert!(message.contains("old-value"));
                assert!(message.contains("new-value"));
            }
            _ => panic!("Expected SchemaError for default value change"),
        }
    }

    #[test]
    fn test_added_namespace_passes() {
        let (old_dir, new_dir) = setup_dirs();

        let mut props = serde_json::Map::new();
        props.insert("key1".to_string(), property("string", json!("test")));
        let schema = build_schema(props);

        create_schema(&old_dir, "test", &schema);
        create_schema(&new_dir, "test", &schema);
        create_schema(&new_dir, "new-namespace", &schema);

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_added_property_passes() {
        let (old_dir, new_dir) = setup_dirs();

        // Build old schema with one property
        let mut old_props = serde_json::Map::new();
        old_props.insert("key1".to_string(), property("string", json!("test")));
        let old_schema = build_schema(old_props);

        // Modify schema - add key2
        let new_schema = modify_schema(&old_schema, |props| {
            props.insert("key2".to_string(), property("integer", json!(42)));
        });

        create_schema(&old_dir, "test", &old_schema);
        create_schema(&new_dir, "test", &new_schema);

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_integer_to_number_type_change_fails() {
        let (old_dir, new_dir) = setup_dirs();

        // Build old schema with integer type
        let mut props = serde_json::Map::new();
        props.insert("key1".to_string(), property("integer", json!(42)));
        let old_schema = build_schema(props);

        // Modify schema - change to number type
        let new_schema = modify_schema(&old_schema, |props| {
            props.insert("key1".to_string(), property("number", json!(42.0)));
        });

        create_schema(&old_dir, "test", &old_schema);
        create_schema(&new_dir, "test", &new_schema);

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_err());
        match result {
            Err(ValidationError::SchemaError { message, .. }) => {
                assert!(message.contains("type changed from 'integer' to 'number'"));
            }
            _ => panic!("Expected SchemaError for integer to number change"),
        }
    }

    #[test]
    fn test_multiple_namespaces() {
        let (old_dir, new_dir) = setup_dirs();

        // Build schema1 for ns1
        let mut props1 = serde_json::Map::new();
        props1.insert("key1".to_string(), property("string", json!("test")));
        let schema1 = build_schema(props1);

        // Build schema2 for ns2
        let mut props2 = serde_json::Map::new();
        props2.insert("key2".to_string(), property("boolean", json!(true)));
        let schema2 = build_schema(props2);

        create_schema(&old_dir, "ns1", &schema1);
        create_schema(&old_dir, "ns2", &schema2);
        create_schema(&new_dir, "ns1", &schema1);
        create_schema(&new_dir, "ns2", &schema2);

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_ok());
    }
}
