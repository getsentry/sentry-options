//! Integration tests for override_options validation.
//!
//! Uses the sentry-options-testing schema from sentry-options/schemas/
//! to replicate real application behavior.
//!
//! Requires SENTRY_OPTIONS_DIR to be set to an absolute path
//! (e.g., in .envrc or CI config).

use sentry_options::testing::{ensure_initialized, override_options};
use sentry_options::{OptionsError, options};
use serde_json::json;

#[test]
fn test_override_options_validates_and_intercepts_get() {
    let _guard = override_options(&[("sentry-options-testing", "int-option", json!(999))]).unwrap();

    let opts = options("sentry-options-testing");
    assert_eq!(opts.get("int-option").unwrap(), json!(999));
}

#[test]
fn test_override_options_restores_on_drop() {
    ensure_initialized().unwrap();
    let opts = options("sentry-options-testing");

    // Value from values.json
    assert_eq!(opts.get("int-option").unwrap(), json!(123));

    {
        let _guard =
            override_options(&[("sentry-options-testing", "int-option", json!(999))]).unwrap();
        assert_eq!(opts.get("int-option").unwrap(), json!(999));
    }

    // Back to original after guard drops
    assert_eq!(opts.get("int-option").unwrap(), json!(123));
}

#[test]
fn test_override_options_rejects_unknown_namespace() {
    let result = override_options(&[("unknown_namespace", "key", json!(true))]);

    assert!(matches!(result, Err(OptionsError::UnknownNamespace(_))));
}

#[test]
fn test_override_options_rejects_unknown_key() {
    let result = override_options(&[("sentry-options-testing", "nonexistent-key", json!(true))]);

    assert!(matches!(result, Err(OptionsError::Schema(_))));
}

#[test]
fn test_override_options_rejects_wrong_type_bool() {
    // bool-option is boolean, passing string should fail
    let result =
        override_options(&[("sentry-options-testing", "bool-option", json!("not a bool"))]);

    assert!(matches!(result, Err(OptionsError::Schema(_))));
}

#[test]
fn test_override_options_rejects_wrong_type_int() {
    // int-option is integer, passing string should fail
    let result = override_options(&[("sentry-options-testing", "int-option", json!("not an int"))]);
    assert!(matches!(result, Err(OptionsError::Schema(_))));

    // int-option is integer, passing float should fail
    let result = override_options(&[("sentry-options-testing", "int-option", json!(3.5))]);
    assert!(matches!(result, Err(OptionsError::Schema(_))));
}

#[test]
fn test_override_options_accepts_valid_values() {
    // Valid boolean
    let result = override_options(&[("sentry-options-testing", "bool-option", json!(true))]);
    assert!(result.is_ok());

    // Valid integer
    let result = override_options(&[("sentry-options-testing", "int-option", json!(500))]);
    assert!(result.is_ok());

    // Valid number (float)
    let result = override_options(&[("sentry-options-testing", "float-option", json!(0.75))]);
    assert!(result.is_ok());

    // Valid string
    let result = override_options(&[(
        "sentry-options-testing",
        "string-option",
        json!("test-name"),
    )]);
    assert!(result.is_ok());
}

#[test]
fn test_override_options_multiple_overrides() {
    ensure_initialized().unwrap();
    let opts = options("sentry-options-testing");

    {
        let _guard = override_options(&[
            ("sentry-options-testing", "bool-option", json!(true)),
            ("sentry-options-testing", "int-option", json!(1000)),
            ("sentry-options-testing", "float-option", json!(0.9)),
        ])
        .unwrap();

        assert_eq!(opts.get("bool-option").unwrap(), json!(true));
        assert_eq!(opts.get("int-option").unwrap(), json!(1000));
        assert_eq!(opts.get("float-option").unwrap(), json!(0.9));
    }

    // All back to values.json values
    assert_eq!(opts.get("bool-option").unwrap(), json!(false));
    assert_eq!(opts.get("int-option").unwrap(), json!(123));
    assert_eq!(opts.get("float-option").unwrap(), json!(1.2));
}

#[test]
fn test_override_options_fails_atomically() {
    // Try to set multiple overrides where one is invalid
    let result = override_options(&[
        ("sentry-options-testing", "bool-option", json!(true)), // valid
        ("sentry-options-testing", "int-option", json!("invalid")), // invalid type
    ]);

    // Should fail
    assert!(result.is_err());

    // First override should NOT have been applied (atomic failure)
    let opts = options("sentry-options-testing");
    assert_eq!(opts.get("bool-option").unwrap(), json!(false));
}

#[test]
fn test_override_options_nested_guards() {
    {
        let _outer =
            override_options(&[("sentry-options-testing", "int-option", json!(200))]).unwrap();

        let opts = options("sentry-options-testing");
        assert_eq!(opts.get("int-option").unwrap(), json!(200));

        {
            let _inner =
                override_options(&[("sentry-options-testing", "int-option", json!(300))]).unwrap();
            assert_eq!(opts.get("int-option").unwrap(), json!(300));
        }

        // Inner guard dropped, back to outer override
        assert_eq!(opts.get("int-option").unwrap(), json!(200));
    }

    // Outer guard dropped, back to original
    let opts = options("sentry-options-testing");
    assert_eq!(opts.get("int-option").unwrap(), json!(123));
}
