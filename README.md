# sentry-options

Sentry's internal runtime option and feature-flag platform. Options are defined in your repo and values are set in `sentry-options-automator`. Multiple layers of validation, hot-reloaded, and minimal infrastructure. Awesome.

`sentry-options` was built to replace our legacy database-backed options with a straightforward config file (in the form of a mounted Kubernetes ConfigMap). This lowers infrastructure complexity, makes the code simpler to understand, and dramatically lowers upper percentile (i.e. P90) latency. `sentry-options` is also Rust native, supporting both Rust and Python client libraries out of the box.

## Documentation Reference


| Doc                                       | What it covers                                                            |
| ----------------------------------------- | ------------------------------------------------------------------------- |
| [Setup](./docs/setup.md)                  | Add `sentry-options` to a new repo                                        |
| [Working with options](./docs/options.md) | Adding/deleting options, using options, setting option values, testing    |
| [Architecture](./docs/architecture.md)    | Technical reference: schema format, values & targets, runtime model, feature-flag evaluation |


## Quick Reference

> See [Working with options](./docs/options.md) for a more detailed guide

### Python

```python
from sentry_options import init, options

init()                          # once, early in startup

opts = options('seer')          # your namespace
if opts.get('feature.enabled'):
    rate = opts.get('feature.rate_limit')
    print(f"Feature enabled with rate limit {rate}")
```

### Rust

```rust
use sentry_options::{init, options};

fn main() -> anyhow::Result<()> {
    init()?;                        // once, early in startup

    let opts = options("seer")?;     // your namespace
    if opts.get("feature.enabled")? == serde_json::json!(true) {
        let rate = opts.get("feature.rate_limit")?;
        println!("Feature enabled with rate limit {}", rate);
    }
    Ok(())
}
```
