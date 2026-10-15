//! Percentage based rollout helpers governed by numeric options.
//!
//! These are ports of `sentry.options.rollout`. The governing option should be
//! a `number` option holding a rate between `0.0` (nobody) and `1.0` (everyone).
//!
//! ```rust,ignore
//! use sentry_options::rollout::{in_random_rollout, in_rollout_group};
//!
//! if in_random_rollout("seer", "sample-rate") {
//!     // ...
//! }
//! if in_rollout_group("seer", "org-rollout-rate", org_id) {
//!     // ...
//! }
//! ```

use md5::{Digest, Md5};

use crate::{OptionsError, Result, options};

const BUCKETS: i128 = 100_000;

/// A key used to place an operation into a deterministic rollout group.
///
/// Integer keys are bucketed directly. String keys are hashed with MD5 so that
/// they are bucketed identically to the Python implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RolloutKey<'a> {
    Int(i128),
    Str(&'a str),
}

macro_rules! impl_int_rollout_key {
    ($($t:ty),*) => {
        $(
            impl From<$t> for RolloutKey<'_> {
                fn from(value: $t) -> Self {
                    RolloutKey::Int(value as i128)
                }
            }
        )*
    };
}

impl_int_rollout_key!(i8, i16, i32, i64, i128, u8, u16, u32, u64, isize, usize);

impl<'a> From<&'a str> for RolloutKey<'a> {
    fn from(value: &'a str) -> Self {
        RolloutKey::Str(value)
    }
}

impl<'a> From<&'a String> for RolloutKey<'a> {
    fn from(value: &'a String) -> Self {
        RolloutKey::Str(value.as_str())
    }
}

/// Determine if the current operation is in a random % based rollout group
/// governed by the option `option_name` in `namespace`.
pub fn in_random_rollout(namespace: &str, option_name: &str) -> bool {
    if let Ok(rate) = rollout_rate(namespace, option_name) {
        fastrand::f64() < rate
    } else {
        false
    }
}

/// Determine if `key` is in a deterministic % based rollout group governed by
/// the option `option_name` in `namespace`.
pub fn in_rollout_group<'a>(
    namespace: &str,
    option_name: &str,
    key: impl Into<RolloutKey<'a>>,
) -> bool {
    if let Ok(rate) = rollout_rate(namespace, option_name) {
        bucket(key.into()) < rate
    } else {
        false
    }
}

fn rollout_rate(namespace: &str, option_name: &str) -> Result<f64> {
    options(namespace)?
        .get(option_name)?
        .as_f64()
        .ok_or_else(|| OptionsError::InvalidType {
            namespace: namespace.to_string(),
            key: option_name.to_string(),
            expected: "number",
        })
}

/// Map a key onto `[0, 1)` in steps of 1/100000.
fn bucket(key: RolloutKey<'_>) -> f64 {
    let remainder = match key {
        RolloutKey::Int(value) => value.rem_euclid(BUCKETS),
        RolloutKey::Str(value) => {
            let digest = Md5::digest(value.as_bytes());
            let int_key = u128::from_be_bytes(digest.into());
            (int_key % BUCKETS as u128) as i128
        }
    };
    remainder as f64 / BUCKETS as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_int_keys() {
        assert_eq!(bucket(0.into()), 0.0);
        assert_eq!(bucket(1.into()), 0.00001);
        assert_eq!(bucket(50_000.into()), 0.5);
        assert_eq!(bucket(100_000.into()), 0.0);
        assert_eq!(bucket(123_456.into()), 0.23456);
    }

    #[test]
    fn test_bucket_negative_int_matches_python_modulo() {
        // Python: -5 % 100000 == 99995
        assert_eq!(bucket((-5i64).into()), 0.99995);
    }

    #[test]
    fn test_bucket_large_unsigned_int() {
        // Python: (2**64 - 1) % 100000 == 51615
        assert_eq!(bucket(u64::MAX.into()), 0.51615);
    }

    #[test]
    fn test_bucket_string_keys_match_python() {
        // Expected values from int(hashlib.md5(key).hexdigest(), 16) % 100000
        assert_eq!(bucket("hello".into()), 0.22994);
        assert_eq!(bucket("org:1".into()), 0.091);
        assert_eq!(bucket("sentry".into()), 0.18128);
        assert_eq!(bucket("".into()), 0.78366);

        let owned = String::from("hello");
        assert_eq!(bucket((&owned).into()), 0.22994);
    }
}
