//! Integration test for the global `refresh()`.
//!
//! Lives in its own test binary because it initializes the process-wide store
//! from a temporary directory with refresh-on-read disabled.

use std::fs;

use sentry_options::{Options, OptionsError, options, refresh};
use serde_json::json;
use tempfile::TempDir;

const SCHEMA: &str = r#"{
    "version": "1.0",
    "type": "object",
    "properties": {
        "enabled": {"type": "boolean", "default": false, "description": "Enable feature"}
    }
}"#;

#[test]
fn global_refresh_reloads_with_refresh_on_read_disabled() {
    assert!(matches!(refresh(), Err(OptionsError::NotInitialized)));

    let temp = TempDir::new().unwrap();
    let schema_dir = temp.path().join("schemas/test");
    let values_dir = temp.path().join("values/test");
    fs::create_dir_all(&schema_dir).unwrap();
    fs::create_dir_all(&values_dir).unwrap();
    fs::write(schema_dir.join("schema.json"), SCHEMA).unwrap();
    let values_file = values_dir.join("values.json");
    fs::write(&values_file, r#"{"options": {"enabled": true}}"#).unwrap();

    Options::builder()
        .with_directory(temp.path())
        .with_refresh_threshold(None)
        .init()
        .unwrap();
    let opts = options("test").unwrap();
    assert_eq!(opts.get("enabled").unwrap(), json!(true));

    fs::write(&values_file, r#"{"options": {"enabled": false}}"#).unwrap();

    // With refresh-on-read disabled, reads never pick up the change.
    assert_eq!(opts.get("enabled").unwrap(), json!(true));
    // The global refresh does.
    assert!(refresh().unwrap());
    assert_eq!(opts.get("enabled").unwrap(), json!(false));
    // Nothing changed since the last refresh.
    assert!(!refresh().unwrap());
}
