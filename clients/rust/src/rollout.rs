//! Percentage based rollout helpers governed by numeric options.
//!
//! These are ports of `sentry.options.rollout`. The governing option should be
//! a `number` option holding a rate between `0.0` (nobody) and `1.0` (everyone).
//!
//! ```rust,ignore
//! use sentry_options::options;
//!
//! let opts = options("seer")?;
//! if opts.in_random_rollout("sample-rate") {
//!     // ...
//! }
//! if opts.in_rollout_group("org-rollout-rate", org_id) {
//!     // ...
//! }
//! ```

use md5::{Digest, Md5};

use crate::Options;

pub const BUCKETS: i128 = 100_000;

/// A key used to place an operation into a deterministic rollout group.
///
/// Integer keys are bucketed directly. String keys are hashed with MD5 so that
/// they are bucketed identically to the Python implementation.
pub trait RolloutKey {
    /// Map this key onto `[0, 1)` in steps of 1/100000.
    fn bucket(&self) -> f64;
}

macro_rules! impl_int_rollout_key {
    ($($t:ty),*) => {
        $(
            impl RolloutKey for $t {
                fn bucket(&self) -> f64 {
                    (*self as i128).rem_euclid(BUCKETS) as f64 / BUCKETS as f64
                }
            }
        )*
    };
}

impl_int_rollout_key!(i8, i16, i32, i64, i128, u8, u16, u32, u64, isize, usize);

impl RolloutKey for str {
    fn bucket(&self) -> f64 {
        let digest = Md5::digest(self.as_bytes());
        let int_key = u128::from_be_bytes(digest.into());
        (int_key % BUCKETS as u128) as f64 / BUCKETS as f64
    }
}

impl RolloutKey for String {
    fn bucket(&self) -> f64 {
        self.as_str().bucket()
    }
}

impl<T: RolloutKey + ?Sized> RolloutKey for &T {
    fn bucket(&self) -> f64 {
        (**self).bucket()
    }
}

impl Options {
    /// Determine if the current operation is in a random % based rollout
    /// group governed by the option `option_name` in `namespace`.
    ///
    /// Returns `false` if the option is unknown or not a number.
    pub fn in_random_rollout(&self, namespace: &str, option_name: &str) -> bool {
        fastrand::f64() < self.rollout_rate(namespace, option_name)
    }

    /// Determine if `key` is in a deterministic % based rollout group governed
    /// by the option `option_name` in `namespace`.
    ///
    /// Returns `false` if the option is unknown or not a number.
    pub fn in_rollout_group(
        &self,
        namespace: &str,
        option_name: &str,
        key: impl RolloutKey,
    ) -> bool {
        key.bucket() < self.rollout_rate(namespace, option_name)
    }

    /// Unknown or non-numeric options are treated as a 0% rollout.
    fn rollout_rate(&self, namespace: &str, option_name: &str) -> f64 {
        self.get(namespace, option_name)
            .ok()
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bucket_int_keys() {
        assert_eq!(0i64.bucket(), 0.0);
        assert_eq!(1i64.bucket(), 0.00001);
        assert_eq!(50_000i64.bucket(), 0.5);
        assert_eq!(100_000i64.bucket(), 0.0);
        assert_eq!(123_456i64.bucket(), 0.23456);
    }

    #[test]
    fn test_bucket_negative_int_matches_python_modulo() {
        // Python: -5 % 100000 == 99995
        assert_eq!((-5i64).bucket(), 0.99995);
    }

    #[test]
    fn test_bucket_large_unsigned_int() {
        // Python: (2**64 - 1) % 100000 == 51615
        assert_eq!(u64::MAX.bucket(), 0.51615);
    }

    #[test]
    fn test_bucket_string_keys_match_python() {
        // Expected values from int(hashlib.md5(key).hexdigest(), 16) % 100000
        assert_eq!("hello".bucket(), 0.22994);
        assert_eq!("org:1".bucket(), 0.091);
        assert_eq!("sentry".bucket(), 0.18128);
        assert_eq!("".bucket(), 0.78366);

        let owned = String::from("hello");
        assert_eq!(owned.bucket(), 0.22994);
    }
}
