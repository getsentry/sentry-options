use sentry_options_validation::{
    OptionMetadata, SchemaRegistry, ValidationError, ValidationResult,
};
use std::collections::HashMap;
use std::fmt;
use std::path::Path;

#[derive(Debug)]
enum SchemaChangeAction {
    NamespaceAdded(String),
    NamespaceRemoved(String),
    OptionAdded {
        namespace: String,
        name: String,
    },
    OptionRemoved {
        namespace: String,
        name: String,
    },
    TypeChanged {
        context: String, // namespace.option
        old: String,
        new: String,
    },
    DefaultChanged {
        context: String, // namespace.option
        old: String,
        new: String,
    },
}

impl fmt::Display for SchemaChangeAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaChangeAction::NamespaceAdded(name) => {
                write!(f, "+ Namespace:\t {}", name)
            }
            SchemaChangeAction::NamespaceRemoved(name) => {
                write!(f, "- Namespace:\t {}", name)
            }
            SchemaChangeAction::OptionAdded { namespace, name } => {
                write!(f, "+ Option:\t {}.{}", namespace, name)
            }
            SchemaChangeAction::OptionRemoved { namespace, name } => {
                write!(f, "- Option:\t {}.{}", namespace, name)
            }
            SchemaChangeAction::TypeChanged { context, old, new } => {
                write!(f, "~ Type:\t\t {}: {} -> {}", context, old, new)
            }
            SchemaChangeAction::DefaultChanged { context, old, new } => {
                write!(f, "~ Default:\t {}: {} -> {}", context, old, new)
            }
        }
    }
}

/// Compare two schema files and validate no breaking changes occurred
fn compare_schemas(
    namespace: &str,
    old_options: &HashMap<String, OptionMetadata>,
    new_options: &HashMap<String, OptionMetadata>,
    changelog: &mut Vec<SchemaChangeAction>,
    errors: &mut Vec<ValidationError>,
) {
    let mut removed_options = Vec::new();
    let mut modified_options = Vec::new();
    let mut added_options = Vec::new();

    let mut old_keys: Vec<_> = old_options.keys().collect();
    old_keys.sort();

    for key in old_keys {
        let old_meta = &old_options[key];
        // 3. removing options
        let Some(new_meta) = new_options.get(key) else {
            removed_options.push(SchemaChangeAction::OptionRemoved {
                namespace: namespace.to_string(),
                name: key.to_string(),
            });
            errors.push(ValidationError::SchemaError {
                file: format!("schemas/{}/schema.json", namespace).into(),
                message: format!("Option '{}.{}' was removed", namespace, key),
            });
            continue;
        };

        // 5. changing option type
        if old_meta.option_type != new_meta.option_type {
            modified_options.push(SchemaChangeAction::TypeChanged {
                context: format!("{}.{}", namespace, key),
                old: old_meta.option_type.clone(),
                new: new_meta.option_type.clone(),
            });
            errors.push(ValidationError::SchemaError {
                file: format!("schemas/{}/schema.json", namespace).into(),
                message: format!(
                    "Option '{}.{}' type changed from '{}' to '{}'",
                    namespace, key, old_meta.option_type, new_meta.option_type
                ),
            });
        }

        // 6. changing option default
        if old_meta.default != new_meta.default {
            modified_options.push(SchemaChangeAction::DefaultChanged {
                context: format!("{}.{}", namespace, key),
                old: old_meta.default.to_string(),
                new: new_meta.default.to_string(),
            });
            errors.push(ValidationError::SchemaError {
                file: format!("schemas/{}/schema.json", namespace).into(),
                message: format!(
                    "Option '{}.{}' default value changed from {} to {}",
                    namespace, key, old_meta.default, new_meta.default
                ),
            });
        }
    }

    // 2. adding new options
    let mut new_keys: Vec<_> = new_options.keys().collect();
    new_keys.sort();
    for key in new_keys {
        if !old_options.contains_key(key) {
            added_options.push(SchemaChangeAction::OptionAdded {
                namespace: namespace.to_string(),
                name: key.to_string(),
            });
        }
    }

    changelog.extend(removed_options);
    changelog.extend(modified_options);
    changelog.extend(added_options);
}

/// Compares 2 schema folders as old and new versions of a repo's options.
/// Validates that any changes made are allowed, otherwise returns an error.
/// Also validates the schemas themselves.
///
/// # Allowed changes:
/// 1. Adding new namespaces
/// 2. Adding new options
///
/// # Forbidden changes (will error):
/// 3. Removing options
/// 4. Removing namespaces
/// 5. Changing option types
/// 6. Changing default values
pub fn detect_changes(
    old_dir: &Path,
    new_dir: &Path,
    repo_name: &str,
    quiet: bool,
) -> ValidationResult<()> {
    let old_registry = SchemaRegistry::from_directory(old_dir)?;
    let new_registry = SchemaRegistry::from_directory(new_dir)?;

    let old_schemas = old_registry.schemas();
    let new_schemas = new_registry.schemas();

    let mut changelog = Vec::new();
    let mut errors = Vec::new();

    // Validate namespace prefixes in new schemas
    let expected_prefix = format!("{}-", repo_name);
    let mut new_namespace_names_sorted: Vec<_> = new_schemas.keys().collect();
    new_namespace_names_sorted.sort();

    for namespace in new_namespace_names_sorted {
        let is_exact_match = namespace == repo_name;
        let has_valid_prefix = namespace.starts_with(&expected_prefix);

        if !is_exact_match && !has_valid_prefix {
            errors.push(ValidationError::SchemaError {
                file: format!("schemas/{}/schema.json", namespace).into(),
                message: format!(
                    "Namespace '{}' is invalid. Expected either '{}' or '{}*' (e.g., '{}-testing')",
                    namespace, repo_name, expected_prefix, repo_name
                ),
            });
        }
    }

    let mut old_namespace_names: Vec<_> = old_schemas.keys().collect();
    old_namespace_names.sort();
    for namespace in old_namespace_names {
        let old_schema = &old_schemas[namespace];
        let Some(new_schema) = new_schemas.get(namespace) else {
            // 4. removing namespaces
            changelog.push(SchemaChangeAction::NamespaceRemoved(namespace.to_string()));
            errors.push(ValidationError::SchemaError {
                file: format!("schemas/{}/schema.json", namespace).into(),
                message: format!("Namespace '{}' was removed", namespace),
            });
            continue;
        };

        compare_schemas(
            namespace,
            &old_schema.options,
            &new_schema.options,
            &mut changelog,
            &mut errors,
        );
    }

    // 1. adding new namespaces
    let mut new_namespace_names: Vec<_> = new_schemas.keys().collect();
    new_namespace_names.sort();
    for namespace in new_namespace_names {
        if !old_schemas.contains_key(namespace) {
            changelog.push(SchemaChangeAction::NamespaceAdded(namespace.to_string()));
        }
    }

    if !quiet {
        println!("Schema Changes:");
        if changelog.is_empty() {
            println!("\tNo changes");
        }
        for change in changelog {
            println!("\t{}", change);
        }
    }

    if !errors.is_empty() {
        println!("Errors:");
        for error in &errors {
            println!("\t{}", error);
        }
        return Err(ValidationError::ValidationErrors(errors));
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

    /// Helper to assert that an error list contains a message with the given text
    fn assert_error_contains(errors: &[ValidationError], text: &str) {
        let found = errors.iter().any(|e| e.to_string().contains(text));
        assert!(
            found,
            "Expected error containing '{}' not found in errors",
            text
        );
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

        let result = detect_changes(old_dir.path(), new_dir.path(), "test", true);
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

        let result = detect_changes(old_dir.path(), new_dir.path(), "test", true);
        assert!(result.is_err());
        match result {
            Err(ValidationError::ValidationErrors(errors)) => {
                assert_eq!(errors.len(), 1);
                assert_error_contains(&errors, "Namespace 'test' was removed");
            }
            _ => panic!("Expected ValidationErrors for removed namespace"),
        }
    }

    #[test]
    fn test_removed_option_fails() {
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

        let result = detect_changes(old_dir.path(), new_dir.path(), "test", true);
        assert!(result.is_err());
        match result {
            Err(ValidationError::ValidationErrors(errors)) => {
                assert_eq!(errors.len(), 1);
                assert_error_contains(&errors, "Option 'test.key2' was removed");
            }
            _ => panic!("Expected ValidationErrors for removed option"),
        }
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

        let result = detect_changes(old_dir.path(), new_dir.path(), "test", true);
        assert!(result.is_err());
        match result {
            Err(ValidationError::ValidationErrors(errors)) => {
                assert_eq!(errors.len(), 2); // Type change + default change
                assert_error_contains(&errors, "type changed from 'string' to 'integer'");
            }
            _ => panic!("Expected ValidationErrors for type change"),
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

        let result = detect_changes(old_dir.path(), new_dir.path(), "test", true);
        assert!(result.is_err());
        match result {
            Err(ValidationError::ValidationErrors(errors)) => {
                assert_eq!(errors.len(), 1);
                assert_error_contains(&errors, "default value changed");
                assert_error_contains(&errors, "old-value");
                assert_error_contains(&errors, "new-value");
            }
            _ => panic!("Expected ValidationErrors for default value change"),
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
        create_schema(&new_dir, "test-new", &schema);

        let result = detect_changes(old_dir.path(), new_dir.path(), "test", true);
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

        let result = detect_changes(old_dir.path(), new_dir.path(), "test", true);
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

        let result = detect_changes(old_dir.path(), new_dir.path(), "test", true);
        assert!(result.is_err());
        match result {
            Err(ValidationError::ValidationErrors(errors)) => {
                assert_eq!(errors.len(), 2); // Type change + default change
                assert_error_contains(&errors, "type changed from 'integer' to 'number'");
                assert_error_contains(&errors, "default value changed from 42 to 42.0");
            }
            _ => panic!("Expected ValidationErrors for integer to number change"),
        }
    }

    #[test]
    fn test_multiple_namespaces() {
        let (old_dir, new_dir) = setup_dirs();

        // Build schema1 for test-ns1
        let mut options1 = serde_json::Map::new();
        options1.insert("key1".to_string(), option("string", json!("test")));
        let schema1 = build_schema(options1);

        // Build schema2 for test-ns2
        let mut options2 = serde_json::Map::new();
        options2.insert("key2".to_string(), option("boolean", json!(true)));
        let schema2 = build_schema(options2);

        create_schema(&old_dir, "test-ns1", &schema1);
        create_schema(&old_dir, "test-ns2", &schema2);
        create_schema(&new_dir, "test-ns1", &schema1);
        create_schema(&new_dir, "test-ns2", &schema2);

        let result = detect_changes(old_dir.path(), new_dir.path(), "test", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_namespace_prefix_exact_match_passes() {
        let (old_dir, new_dir) = setup_dirs();

        let mut options = serde_json::Map::new();
        options.insert("key1".to_string(), option("string", json!("test")));
        let schema = build_schema(options);

        create_schema(&old_dir, "myrepo", &schema);
        create_schema(&new_dir, "myrepo", &schema);

        let result = detect_changes(old_dir.path(), new_dir.path(), "myrepo", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_namespace_prefix_with_suffix_passes() {
        let (old_dir, new_dir) = setup_dirs();

        let mut options = serde_json::Map::new();
        options.insert("key1".to_string(), option("string", json!("test")));
        let schema = build_schema(options);

        create_schema(&old_dir, "myrepo-testing", &schema);
        create_schema(&new_dir, "myrepo-testing", &schema);

        let result = detect_changes(old_dir.path(), new_dir.path(), "myrepo", true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_namespace_prefix_invalid_fails() {
        let (old_dir, new_dir) = setup_dirs();

        let mut options = serde_json::Map::new();
        options.insert("key1".to_string(), option("string", json!("test")));
        let schema = build_schema(options);

        create_schema(&new_dir, "other-testing", &schema);

        let result = detect_changes(old_dir.path(), new_dir.path(), "myrepo", true);
        assert!(result.is_err());
        match result {
            Err(ValidationError::ValidationErrors(errors)) => {
                assert_error_contains(&errors, "is invalid. Expected either");
            }
            _ => panic!("Expected ValidationErrors for invalid namespace prefix"),
        }
    }

    #[test]
    fn test_namespace_prefix_no_prefix_fails() {
        let (old_dir, new_dir) = setup_dirs();

        let mut options = serde_json::Map::new();
        options.insert("key1".to_string(), option("string", json!("test")));
        let schema = build_schema(options);

        create_schema(&new_dir, "testing", &schema);

        let result = detect_changes(old_dir.path(), new_dir.path(), "myrepo", true);
        assert!(result.is_err());
        match result {
            Err(ValidationError::ValidationErrors(errors)) => {
                assert_error_contains(&errors, "is invalid. Expected either");
            }
            _ => panic!("Expected ValidationErrors for missing namespace prefix"),
        }
    }
}
