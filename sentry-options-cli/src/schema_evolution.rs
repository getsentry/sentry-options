use sentry_options_validation::{
    OptionMetadata, SchemaRegistry, ValidationError, ValidationResult,
};
use std::collections::HashMap;
use std::path::Path;

/// Compare two schema files and validate no breaking changes occurred
fn compare_schemas(
    namespace: &str,
    old_options: &HashMap<String, OptionMetadata>,
    new_options: &HashMap<String, OptionMetadata>,
) -> ValidationResult<()> {
    for (key, old_meta) in old_options {
        // Skip if option was removed (allowed for now)
        let Some(new_meta) = new_options.get(key) else {
            continue;
        };

        // 5. changing option type
        if old_meta.option_type != new_meta.option_type {
            return Err(ValidationError::SchemaError {
                file: format!("schemas/{}/schema.json", namespace).into(),
                message: format!(
                    "Option '{}.{}' type changed from '{}' to '{}'",
                    namespace, key, old_meta.option_type, new_meta.option_type
                ),
            });
        }

        // 6. changing option default
        if old_meta.default != new_meta.default {
            return Err(ValidationError::SchemaError {
                file: format!("schemas/{}/schema.json", namespace).into(),
                message: format!(
                    "Option '{}.{}' default value changed from {} to {}",
                    namespace, key, old_meta.default, new_meta.default
                ),
            });
        }
    }

    Ok(())
}

/// Compares 2 schema folders as old and new versions of a repo's options.
/// Assumes the schemas themselves are already validated.
/// Validates that any changes made are allowed, otherwise returns an error.
///
/// # Allowed changes:
/// 1. Adding new namespaces
/// 2. Adding new options
///
/// # Allowed for now:
/// 3. Removing options
///
/// # Forbidden changes (will error):
/// 4. Removing namespaces
/// 5. Changing option types
/// 6. Changing default values
pub fn detect_changes(old_dir: &Path, new_dir: &Path) -> ValidationResult<()> {
    let old_registry = SchemaRegistry::from_directory(old_dir)?;
    let new_registry = SchemaRegistry::from_directory(new_dir)?;

    let old_schemas = old_registry.schemas();
    let new_schemas = new_registry.schemas();

    for (namespace, old_schema) in old_schemas {
        let new_schema = new_schemas
            .get(namespace)
            // 4. removing namespaces
            .ok_or_else(|| ValidationError::SchemaError {
                file: format!("schemas/{}/schema.json", namespace).into(),
                message: format!("Namespace '{}' was removed", namespace),
            })?;

        compare_schemas(namespace, &old_schema.options, &new_schema.options)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::fs;
    use tempfile::TempDir;

    /// Helper to set up test directories
    fn setup_dirs() -> (TempDir, TempDir) {
        (TempDir::new().unwrap(), TempDir::new().unwrap())
    }

    /// Helper to build an option definition
    fn option(option_type: &str, default: Value) -> Value {
        json!({
            "type": option_type,
            "default": default,
            "description": "Test option"
        })
    }

    /// Helper to build a schema with given options
    fn build_schema(options: serde_json::Map<String, Value>) -> Value {
        json!({
            "version": "1.0",
            "type": "object",
            "properties": options
        })
    }

    /// Helper to create a schema directory with a schema
    fn create_schema(temp_dir: &TempDir, namespace: &str, schema: &Value) {
        let schema_dir = temp_dir.path().join(namespace);
        fs::create_dir_all(&schema_dir).unwrap();
        let schema_file = schema_dir.join("schema.json");
        fs::write(&schema_file, serde_json::to_string_pretty(schema).unwrap()).unwrap();
    }

    /// Helper to modify a schema by cloning and applying changes to options
    fn modify_schema(schema: &Value, f: impl FnOnce(&mut serde_json::Map<String, Value>)) -> Value {
        let mut new_schema = schema.clone();
        if let Some(opts) = new_schema
            .get_mut("properties")
            .and_then(|p| p.as_object_mut())
        {
            f(opts);
        }
        new_schema
    }

    #[test]
    fn test_identical_schemas_pass() {
        let (old_dir, new_dir) = setup_dirs();

        let mut options = serde_json::Map::new();
        options.insert("key1".to_string(), option("string", json!("test")));
        let schema = build_schema(options);

        create_schema(&old_dir, "test", &schema);
        create_schema(&new_dir, "test", &schema);

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_removed_namespace_fails() {
        let (old_dir, new_dir) = setup_dirs();

        let mut options = serde_json::Map::new();
        options.insert("key1".to_string(), option("string", json!("test")));
        let schema = build_schema(options);

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
    fn test_removed_option_passes() {
        let (old_dir, new_dir) = setup_dirs();

        // Build old schema with two options
        let mut old_options = serde_json::Map::new();
        old_options.insert("key1".to_string(), option("string", json!("test")));
        old_options.insert("key2".to_string(), option("integer", json!(42)));
        let old_schema = build_schema(old_options);

        // Modify schema - remove key2
        let new_schema = modify_schema(&old_schema, |options| {
            options.remove("key2");
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
        let mut options = serde_json::Map::new();
        options.insert("key1".to_string(), option("string", json!("test")));
        let old_schema = build_schema(options);

        // Modify schema - change type
        let new_schema = modify_schema(&old_schema, |options| {
            options.insert("key1".to_string(), option("integer", json!(42)));
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
        let mut options = serde_json::Map::new();
        options.insert("key1".to_string(), option("string", json!("old-value")));
        let old_schema = build_schema(options);

        // Modify schema - change default value
        let new_schema = modify_schema(&old_schema, |options| {
            options.insert("key1".to_string(), option("string", json!("new-value")));
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

        let mut options = serde_json::Map::new();
        options.insert("key1".to_string(), option("string", json!("test")));
        let schema = build_schema(options);

        create_schema(&old_dir, "test", &schema);
        create_schema(&new_dir, "test", &schema);
        create_schema(&new_dir, "new-namespace", &schema);

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn test_added_option_passes() {
        let (old_dir, new_dir) = setup_dirs();

        // Build old schema with one option
        let mut old_options = serde_json::Map::new();
        old_options.insert("key1".to_string(), option("string", json!("test")));
        let old_schema = build_schema(old_options);

        // Modify schema - add key2
        let new_schema = modify_schema(&old_schema, |options| {
            options.insert("key2".to_string(), option("integer", json!(42)));
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
        let mut options = serde_json::Map::new();
        options.insert("key1".to_string(), option("integer", json!(42)));
        let old_schema = build_schema(options);

        // Modify schema - change to number type
        let new_schema = modify_schema(&old_schema, |options| {
            options.insert("key1".to_string(), option("number", json!(42.0)));
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
        let mut options1 = serde_json::Map::new();
        options1.insert("key1".to_string(), option("string", json!("test")));
        let schema1 = build_schema(options1);

        // Build schema2 for ns2
        let mut options2 = serde_json::Map::new();
        options2.insert("key2".to_string(), option("boolean", json!(true)));
        let schema2 = build_schema(options2);

        create_schema(&old_dir, "ns1", &schema1);
        create_schema(&old_dir, "ns2", &schema2);
        create_schema(&new_dir, "ns1", &schema1);
        create_schema(&new_dir, "ns2", &schema2);

        let result = detect_changes(old_dir.path(), new_dir.path());
        assert!(result.is_ok());
    }
}
