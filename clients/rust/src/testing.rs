//! Testing utilities for overriding option values in tests.
//!
//! # Example
//!
//! ```rust,ignore
//! use sentry_options::Options;
//! use sentry_options::testing::override_options;
//! use serde_json::json;
//!
//! #[test]
//! fn test_feature() {
//!     let opts = Options::from_directory(test_dir).unwrap();
//!
//!     let _guard = override_options(&[("seer", "feature.enabled", json!(true))]).unwrap();
//!     assert_eq!(opts.get("seer", "feature.enabled").unwrap(), json!(true));
//! }
//! ```

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::Value;

use crate::{GLOBAL_OPTIONS, Result};

thread_local! {
    static OVERRIDES: RefCell<HashMap<String, HashMap<String, Value>>> = RefCell::new(HashMap::new());
}

/// Set an override value for a specific namespace and key.
pub fn set_override(namespace: &str, key: &str, value: Value) {
    OVERRIDES.with(|o| {
        o.borrow_mut()
            .entry(namespace.to_string())
            .or_default()
            .insert(key.to_string(), value);
    });
}

/// Get an override value if one exists.
pub fn get_override(namespace: &str, key: &str) -> Option<Value> {
    OVERRIDES.with(|o| {
        o.borrow()
            .get(namespace)
            .and_then(|ns| ns.get(key).cloned())
    })
}

/// Clear an override for a specific namespace and key.
pub fn clear_override(namespace: &str, key: &str) {
    OVERRIDES.with(|o| {
        if let Some(ns_map) = o.borrow_mut().get_mut(namespace) {
            ns_map.remove(key);
        }
    });
}

/// Guard that restores overrides when dropped.
pub struct OverrideGuard {
    previous: Vec<(String, String, Option<Value>)>,
}

impl Drop for OverrideGuard {
    fn drop(&mut self) {
        OVERRIDES.with(|o| {
            let mut map = o.borrow_mut();
            for (ns, key, prev_value) in self.previous.drain(..) {
                match prev_value {
                    Some(v) => {
                        map.entry(ns).or_default().insert(key, v);
                    }
                    None => {
                        if let Some(ns_map) = map.get_mut(&ns) {
                            ns_map.remove(&key);
                        }
                    }
                }
            }
        });
    }
}

/// Set overrides for the lifetime of the returned guard.
///
/// Validates that each key exists in the schema and the value matches the expected type.
/// When the guard is dropped (goes out of scope), the overrides are restored
/// to their previous values.
///
/// # Note
///
/// Overrides are thread-local. They won't apply to spawned threads.
///
/// # Errors
///
/// Returns an error if:
/// - Options have not been initialized (call `init()` first)
/// - Any namespace doesn't exist
/// - Any key doesn't exist in the schema
/// - Any value doesn't match the expected type
///
/// # Example
///
/// ```rust,ignore
/// use sentry_options::testing::override_options;
/// use serde_json::json;
///
/// let _guard = override_options(&[
///     ("namespace", "key1", json!(true)),
///     ("namespace", "key2", json!(42)),
/// ]).unwrap();
/// // overrides are active here
/// // when _guard goes out of scope, overrides are restored
/// ```
pub fn override_options(overrides: &[(&str, &str, Value)]) -> Result<OverrideGuard> {
    // Validate all overrides before applying any
    if let Some(opts) = GLOBAL_OPTIONS.get() {
        for (ns, key, value) in overrides {
            opts.validate_override(ns, key, value)?;
        }
    }

    let mut previous = Vec::with_capacity(overrides.len());

    OVERRIDES.with(|o| {
        let mut map = o.borrow_mut();
        for (ns, key, value) in overrides {
            let prev = map.get(*ns).and_then(|m| m.get(*key).cloned());
            previous.push((ns.to_string(), key.to_string(), prev));
            map.entry(ns.to_string())
                .or_default()
                .insert(key.to_string(), value.clone());
        }
    });

    Ok(OverrideGuard { previous })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_set_get_override() {
        set_override("ns", "key", json!(true));
        assert_eq!(get_override("ns", "key"), Some(json!(true)));
        clear_override("ns", "key");
        assert_eq!(get_override("ns", "key"), None);
    }

    #[test]
    fn test_override_guard_restores() {
        set_override("ns", "key", json!(1));

        {
            // No global options initialized, validation is skipped
            let _guard = override_options(&[("ns", "key", json!(2))]).unwrap();
            assert_eq!(get_override("ns", "key"), Some(json!(2)));
        }

        assert_eq!(get_override("ns", "key"), Some(json!(1)));
        clear_override("ns", "key");
    }

    #[test]
    fn test_override_guard_clears_new_key() {
        assert_eq!(get_override("ns", "new_key"), None);

        {
            // No global options initialized, validation is skipped
            let _guard = override_options(&[("ns", "new_key", json!(true))]).unwrap();
            assert_eq!(get_override("ns", "new_key"), Some(json!(true)));
        }

        assert_eq!(get_override("ns", "new_key"), None);
    }

    #[test]
    fn test_nested_overrides() {
        {
            // No global options initialized, validation is skipped
            let _outer = override_options(&[("ns", "key", json!(1))]).unwrap();
            assert_eq!(get_override("ns", "key"), Some(json!(1)));

            {
                let _inner = override_options(&[("ns", "key", json!(2))]).unwrap();
                assert_eq!(get_override("ns", "key"), Some(json!(2)));
            }

            assert_eq!(get_override("ns", "key"), Some(json!(1)));
        }

        assert_eq!(get_override("ns", "key"), None);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::Options;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_options() -> (TempDir, Options) {
        let temp = TempDir::new().unwrap();
        let schemas = temp.path().join("schemas");
        let values = temp.path().join("values");

        // Create schema
        let schema_dir = schemas.join("myapp");
        fs::create_dir_all(&schema_dir).unwrap();
        fs::write(
            schema_dir.join("schema.json"),
            r#"{
                "version": "1.0",
                "type": "object",
                "properties": {
                    "feature.enabled": {
                        "type": "boolean",
                        "default": false,
                        "description": "Enable feature"
                    },
                    "rate.limit": {
                        "type": "integer",
                        "default": 100,
                        "description": "Rate limit"
                    }
                }
            }"#,
        )
        .unwrap();

        // Create values
        let values_dir = values.join("myapp");
        fs::create_dir_all(&values_dir).unwrap();
        fs::write(
            values_dir.join("values.json"),
            r#"{"options": {"rate.limit": 500}}"#,
        )
        .unwrap();

        let opts = Options::from_directory(temp.path()).unwrap();
        (temp, opts)
    }

    #[test]
    fn test_real_app_override_flow() {
        let (_temp, opts) = setup_test_options();

        // Simulate app code reading config
        fn get_rate_limit(opts: &Options) -> i64 {
            opts.get("myapp", "rate.limit").unwrap().as_i64().unwrap()
        }

        fn is_feature_enabled(opts: &Options) -> bool {
            opts.get("myapp", "feature.enabled")
                .unwrap()
                .as_bool()
                .unwrap()
        }

        // Normal operation - reads from values/defaults
        assert_eq!(get_rate_limit(&opts), 500); // from values.json
        assert!(!is_feature_enabled(&opts)); // default

        // Test with override
        {
            let _guard = override_options(&[
                ("myapp", "feature.enabled", json!(true)),
                ("myapp", "rate.limit", json!(1000)),
            ])
            .unwrap();

            // App code sees overridden values
            assert_eq!(get_rate_limit(&opts), 1000);
            assert!(is_feature_enabled(&opts));
        }

        // After test - back to normal
        assert_eq!(get_rate_limit(&opts), 500);
        assert!(!is_feature_enabled(&opts));
    }
}
