//! Integration tests for rollout helpers.
//!
//! Uses the sentry-options-testing schema from sentry-options/schemas/.
//!
//! Requires SENTRY_OPTIONS_DIR to be set to an absolute path
//! (e.g., in .envrc or CI config).

use sentry_options::testing::override_options;
use sentry_options::{OptionsError, in_random_rollout, in_rollout_group, init};
use serde_json::json;

const NS: &str = "sentry-options-testing";

#[test]
fn test_in_random_rollout_bounds() {
    init().unwrap();
    {
        let _guard = override_options(&[(NS, "float-option", json!(0.0))]).unwrap();
        assert!((0..100).all(|_| !in_random_rollout(NS, "float-option")));
    }
    {
        let _guard = override_options(&[(NS, "float-option", json!(1.0))]).unwrap();
        assert!((0..100).all(|_| in_random_rollout(NS, "float-option")));
    }
}

#[test]
fn test_in_random_rollout_partial() {
    init().unwrap();
    let _guard = override_options(&[(NS, "float-option", json!(0.5))]).unwrap();
    let hits = (0..10_000)
        .filter(|_| in_random_rollout(NS, "float-option"))
        .count();
    assert!((4_000..6_000).contains(&hits), "hits = {hits}");
}

#[test]
fn test_in_random_rollout_accepts_integer_rate() {
    init().unwrap();
    // A number option may hold an integer such as 1.
    let _guard = override_options(&[(NS, "float-option", json!(1))]).unwrap();
    assert!(in_random_rollout(NS, "float-option"));
}

#[test]
fn test_in_rollout_group_int_keys() {
    init().unwrap();
    let _guard = override_options(&[(NS, "float-option", json!(0.5))]).unwrap();

    assert!(in_rollout_group(NS, "float-option", 0));
    assert!(in_rollout_group(NS, "float-option", 49_999));
    assert!(!in_rollout_group(NS, "float-option", 50_000));
    assert!(!in_rollout_group(NS, "float-option", 99_999));
    assert!(in_rollout_group(NS, "float-option", 100_000u64));
}

#[test]
fn test_in_rollout_group_string_keys() {
    init().unwrap();
    // md5("hello") buckets to 0.22994
    {
        let _guard = override_options(&[(NS, "float-option", json!(0.22995))]).unwrap();
        assert!(in_rollout_group(NS, "float-option", "hello"));
    }
    {
        let _guard = override_options(&[(NS, "float-option", json!(0.22994))]).unwrap();
        assert!(!in_rollout_group(NS, "float-option", "hello"));
    }
}

#[test]
fn test_in_rollout_group_bounds() {
    init().unwrap();
    {
        let _guard = override_options(&[(NS, "float-option", json!(0.0))]).unwrap();
        assert!((0..1000).all(|key| !in_rollout_group(NS, "float-option", key)));
    }
    {
        let _guard = override_options(&[(NS, "float-option", json!(1.0))]).unwrap();
        assert!((0..1000).all(|key| in_rollout_group(NS, "float-option", key)));
    }
}

#[test]
fn test_rollout_rejects_non_numeric_option() {
    init().unwrap();
    assert_eq!(in_random_rollout(NS, "example-option"), false);
    assert_eq!(in_rollout_group(NS, "example-option", 1), false);
}

#[test]
fn test_rollout_unknown_option() {
    init().unwrap();
    assert_eq!(in_rollout_group(NS, "nonexistent-key", 1), false);
    assert_eq!(
        in_random_rollout("unknown-namespace", "float-option"),
        false
    );
}
