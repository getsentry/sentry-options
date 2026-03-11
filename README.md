# sentry-options

The amazing `sentry-options` is Sentry's internal option/feature flag management platform. It offers a reusable way to easily get configuration at run-time into your Kubernetes service.

Backed by volume mounted ConfigMaps, `sentry-options` provides hot-reloadable options with no database overhead.

## Overview

`sentry-options` consists of several components:

### sentry-options-cli

The CLI tool typically run in CI to help validate and generate ConfigMaps.

### sentry-options-validation

A library with common code used in both the CLI and client libraries.

### clients

To make ingesting options as simple as possible, `sentry-options` comes with client libraries in two different languages: Rust and Python, hosted on crates.io and our internal pypi respectively.

### validate-schema

A reusable GitHub workflow that ensures

- Your schema definition is valid
- Any changes you make to the schema follow the [schema evolution rules](./docs/integration-guide.md#schema-evolution-rules)
- Any deleted options are no longer in use in sentry-options-automator

Call this from your repo to ensure your schema is valid before it fails in CI much later on (when you start changing values)

## New repo setup

For a detailed guide on how to set up `sentry-options` in your repo, refer to the [integration guide](./docs/integration-guide.md). This will require changes in:

### **your** service repo (e.g. getsentry)

- Defining your schema (what options do we have?)
- Importing the `validate-schemas` action
- Using the client library in your code

### **sentry-options-automator** repo

- An update to `repos.json`, so values can be validated against the schema
- Defining the values for your option

### **ops** repo

- Updating your k8 cluster to mount your config map

## How to use options in code

If you already have `sentry-options` set up in your repo, you only need to import the client library and use it:

### Python

```py
# index.py
from sentry_options import init, options

# Initialize the library
# Do this *once* early on
init()

# Get options for a specified namespace
opts = options('seer')

# Read the values
# If the option value is not set in the automator repo, it will just use the default
if opts.get('feature.enabled'):
    rate = opts.get('feature.rate_limit')
    print(f"The global rate limit is {rate}")
```

### Rust

```rust
// main.rs
use sentry_options::{init, options};

fn main() -> anyhow::Result<()> {
    // Initialize the library
    // Do this *once* early on
    init()?;

    // Get options for a specified namespace
    let opts = options("seer");

    // Read the values
    // If the option value is not set in the automator repo, it will just use the default
    if opts.get("feature.enabled")?.as_bool().unwrap() {
        let rate = opts.get("feature.rate_limit")?;
        println!("The global rate limit is {}", rate);
    }
}
```

### Local option value testing

To test behaviour when an option value change, you can override your default values with local values.

In a new file in the same directory as your schemas, e.g. `sentry-options/values/{namespace}/`, create a `values.json`:

```json
// values.json
{
  "options": {
    "feature.enabled": true,
    "feature.rate_limit": 200
  }
}
```

Your client libraries will automatically pick up the new values.


### Feature Flags

sentry-options supports rich feature flags, that enable features to be enabled for subsets of your
application's traffic to be exposed to features based on arbitrary logic defined as segments and conditions.
See the [flagpole documentation](https://develop.sentry.dev/backend/application-domains/feature-flags/flagpole/) for
more information.

To check feature flags from python:

```python
from sentry_options import init, features, FeatureContext

init()

context = FeatureContext(
    {"org_id": 123, "user_id": 456, "user_email": "sal@example.org"},
    identity_fields=["user_id"]
)

feature_checker = features("getsentry")
if feature_checker.has("organizations:red-bar", context):
    # User has the feature
```

and from rust:

```rust
use sentry_options::{init, features, FeatureContext};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init()?;
    let mut context = FeatureChecker::new();
    context.insert("user_id", 123);
    context.insert("org_id", 456);
    context.insert("user_email", "sal@example.org");
    context.identity_fields(vec!["user_id"]);

    let feature_checker = features("getsentry");
    if feature_checker.has("organizations:red-bar", context) {
        // User has feature.
    }
}
```
