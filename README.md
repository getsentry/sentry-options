# sentry-options

The amazing `sentry-options` is Sentry's internal option/feature flag management platform. It offers a reusable way to easily get configuration at run-time into your Kubernetes service.

Backed by memory mounted ConfigMaps, `sentry-options` provides hot-reloadable options with no database overhead.

## Overview

`sentry-options` consists of several components:

### sentry-options-cli

The CLI tool typically run in CI to help validate and generate ConfigMaps.

### sentry-options-validation

A library with common code used in both the CLI and client libraries.

### clients

To make ingesting options as simple as possible, `sentry-options` comes with client libraries in two different languages: Rust and Python, hosted on crates.io and our internal pypi respectively.

### validate-schema

A composite GitHub action that ensures

- Your schema definition is valid
- Any changes you make to the schema follow the [schema evolution rules](./docs/integration-guide.md#schema-evolution-rules)

Import this into your repo to ensure your schema is valid before it fails in CI much later on (when you start changing values)

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
