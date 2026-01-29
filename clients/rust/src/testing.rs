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

use crate::{GLOBAL_OPTIONS, OptionsError, Result};

/// Initialize options, ignoring "already initialized" errors.
///
/// This is useful in tests where multiple test functions may call this,
/// but only the first one actually initializes.
///
/// # Errors
///
/// Returns an error if initialization fails for reasons other than
/// already being initialized (e.g., schema loading errors).
///
/// # Example
///
/// ```rust,ignore
/// use sentry_options::testing::ensure_initialized;
///
/// #[test]
/// fn test_something() {
///     ensure_initialized().unwrap();
///     // ... test code
/// }
/// ```
pub fn ensure_initialized() -> Result<()> {
    match crate::init() {
        Ok(()) => Ok(()),
        Err(OptionsError::AlreadyInitialized) => Ok(()),
        Err(e) => Err(e),
    }
}

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
/// Automatically initializes global options if not already initialized.
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
/// - Initialization fails (e.g., schema loading errors)
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
    // Auto-initialize if needed
    ensure_initialized()?;

    // Validate all overrides before applying any
    let opts = GLOBAL_OPTIONS.get().expect("ensure_initialized succeeded");
    for (ns, key, value) in overrides {
        opts.validate_override(ns, key, value)?;
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
    fn test_set_get_clear_override() {
        set_override("ns", "key", json!(true));
        assert_eq!(get_override("ns", "key"), Some(json!(true)));
        clear_override("ns", "key");
        assert_eq!(get_override("ns", "key"), None);
    }

    #[test]
    fn test_override_guard_restores() {
        set_override("sentry-options-testing", "int-option", json!(1));

        {
            let _guard =
                override_options(&[("sentry-options-testing", "int-option", json!(2))]).unwrap();
            assert_eq!(
                get_override("sentry-options-testing", "int-option"),
                Some(json!(2))
            );
        }

        assert_eq!(
            get_override("sentry-options-testing", "int-option"),
            Some(json!(1))
        );
        clear_override("sentry-options-testing", "int-option");
    }

    #[test]
    fn test_override_guard_clears_new_key() {
        assert_eq!(get_override("sentry-options-testing", "bool-option"), None);

        {
            let _guard =
                override_options(&[("sentry-options-testing", "bool-option", json!(true))])
                    .unwrap();
            assert_eq!(
                get_override("sentry-options-testing", "bool-option"),
                Some(json!(true))
            );
        }

        assert_eq!(get_override("sentry-options-testing", "bool-option"), None);
    }

    #[test]
    fn test_nested_overrides() {
        {
            let _outer =
                override_options(&[("sentry-options-testing", "int-option", json!(100))]).unwrap();
            assert_eq!(
                get_override("sentry-options-testing", "int-option"),
                Some(json!(100))
            );

            {
                let _inner =
                    override_options(&[("sentry-options-testing", "int-option", json!(200))])
                        .unwrap();
                assert_eq!(
                    get_override("sentry-options-testing", "int-option"),
                    Some(json!(200))
                );
            }

            assert_eq!(
                get_override("sentry-options-testing", "int-option"),
                Some(json!(100))
            );
        }

        assert_eq!(get_override("sentry-options-testing", "int-option"), None);
    }
}
