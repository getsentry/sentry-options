//! Integration tests for rollout helpers.
//!
//! Uses the sentry-options-testing schema from sentry-options/schemas/.
//!
//! Requires SENTRY_OPTIONS_DIR to be set to an absolute path
//! (e.g., in .envrc or CI config).

use sentry_options::testing::override_options;
use sentry_options::{NamespaceOptions, init, options};
use serde_json::json;

const NS: &str = "sentry-options-testing";

fn testing_options() -> NamespaceOptions {
    init().unwrap();
    options(NS).unwrap()
}

#[test]
fn test_in_random_rollout_bounds() {
    let opts = testing_options();
    {
        let _guard = override_options(&[(NS, "float-option", json!(0.0))]).unwrap();
        assert!((0..100).all(|_| !opts.in_random_rollout("float-option")));
    }
    {
        let _guard = override_options(&[(NS, "float-option", json!(1.0))]).unwrap();
        assert!((0..100).all(|_| opts.in_random_rollout("float-option")));
    }
}

#[test]
fn test_in_random_rollout_partial() {
    let opts = testing_options();
    let _guard = override_options(&[(NS, "float-option", json!(0.5))]).unwrap();
    let hits = (0..10_000)
        .filter(|_| opts.in_random_rollout("float-option"))
        .count();
    assert!((4_000..6_000).contains(&hits), "hits = {hits}");
}

#[test]
fn test_in_random_rollout_accepts_integer_rate() {
    let opts = testing_options();
    // A number option may hold an integer such as 1.
    let _guard = override_options(&[(NS, "float-option", json!(1))]).unwrap();
    assert!(opts.in_random_rollout("float-option"));
}

#[test]
fn test_in_rollout_group_int_keys() {
    let opts = testing_options();
    let _guard = override_options(&[(NS, "float-option", json!(0.5))]).unwrap();

    assert!(opts.in_rollout_group("float-option", 0));
    assert!(opts.in_rollout_group("float-option", 49_999));
    assert!(!opts.in_rollout_group("float-option", 50_000));
    assert!(!opts.in_rollout_group("float-option", 99_999));
    assert!(opts.in_rollout_group("float-option", 100_000u64));
}

#[test]
fn test_in_rollout_group_string_keys() {
    let opts = testing_options();
    // md5("hello") buckets to 0.22994
    {
        let _guard = override_options(&[(NS, "float-option", json!(0.22995))]).unwrap();
        assert!(opts.in_rollout_group("float-option", "hello"));
    }
    {
        let _guard = override_options(&[(NS, "float-option", json!(0.22994))]).unwrap();
        assert!(!opts.in_rollout_group("float-option", "hello"));
    }
}

#[test]
fn test_in_rollout_group_bounds() {
    let opts = testing_options();
    {
        let _guard = override_options(&[(NS, "float-option", json!(0.0))]).unwrap();
        assert!((0..1000).all(|key| !opts.in_rollout_group("float-option", key)));
    }
    {
        let _guard = override_options(&[(NS, "float-option", json!(1.0))]).unwrap();
        assert!((0..1000).all(|key| opts.in_rollout_group("float-option", key)));
    }
}

#[test]
fn test_rollout_non_numeric_option_is_false() {
    let opts = testing_options();
    assert!(!opts.in_random_rollout("example-option"));
    assert!(!opts.in_rollout_group("example-option", 1));

    let _guard = override_options(&[(NS, "bool-option", json!(true))]).unwrap();
    assert!(!opts.in_random_rollout("bool-option"));
    assert!(!opts.in_rollout_group("bool-option", 1));
}

#[test]
fn test_rollout_unknown_option_is_false() {
    let opts = testing_options();
    assert!(!opts.in_random_rollout("nonexistent-key"));
    assert!(!opts.in_rollout_group("nonexistent-key", 1));

    let unknown = options("unknown-namespace").unwrap();
    assert!(!unknown.in_random_rollout("float-option"));
    assert!(!unknown.in_rollout_group("float-option", 1));
}
