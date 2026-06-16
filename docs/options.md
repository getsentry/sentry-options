# Options User Guide

The first half of this document covers runtime options. The second half covers feature flags.

- [Options User Guide](#options-user-guide)
  - [Reading an option](#reading-an-option)
    - [Python](#python)
    - [Rust](#rust)
  - [Testing with options](#testing-with-options)
    - [Python](#python-1)
    - [Rust](#rust-1)
  - [Setting an option value locally](#setting-an-option-value-locally)
  - [Adding an option](#adding-an-option)
  - [Setting an option value](#setting-an-option-value)
  - [Reading a feature flag](#reading-a-feature-flag)
    - [Definitions](#definitions)
    - [Python](#python-2)
    - [Rust](#rust-2)
  - [Testing with feature flags](#testing-with-feature-flags)
    - [Python](#python-3)
    - [Rust](#rust-3)
  - [Setting a feature flag value locally](#setting-a-feature-flag-value-locally)
  - [Adding a feature flag](#adding-a-feature-flag)
  - [Setting a feature flag value](#setting-a-feature-flag-value)

## Reading an option

The value will be taken from the ConfigMap first, but fallback to the default defined in `schema.json` if necessary.

### Python

No casting to satisfy a typechecker is needed assuming you have the precommit stubs generator ([installed](./setup.md#2-add-workflows)).

```python
from sentry_options import options

# hopefully elsewhere in the program else init() was called

opts = options('seer')                  # scope down to your namespace first
timeout = opts.get('inference.timeout') # get the option!
```

### Rust

In Rust, our APIs return `Result` which can be handled any way you wish (we recommend panicking (`.unwrap()`) or propagating the error (`?`) to fail loudly).

```rust
use sentry_options::options;

// hopefully elsewhere in the program init() was called

let opts = options("seer")?;                      // scope down to your namespace first
let timeout = opts.get("inference.timeout")?;     // serde_json::Value

println!("Timeout is {}", timeout);               // Value implements Display so you can directly print
let is_hundred = timeout == 100;                  // Value implements PartialEq so you can directly compare without converting
// However, you cannot do timeout < 100, as Value does not implement PartialOrd. Convert first.
let timeout_i64 = timeout.as_i64().unwrap_or(0);  // converting a Value to a normal rust int
let is_under = timeout_i64 < 100;                 // ok!

/*
#[derive(serde::Deserialize)]
struct IPAddress {
    host: String,
    port: u16,
}
*/
// directly destructure an object
let cfg: IPAddress = serde_json::from_value(opts.get("database.config")?)?;
```

## Testing with options

To override a value locally in a test, use the corresponding guard.

**Make sure to call `init()` at some point before the guard is reached**, e.g. in a setup function.

### Python

Pass in a namespace and key-value pair `override_options(namespace: str, overrides: dict[str, Value])`

```python
from sentry_options import options
from sentry_options.testing import override_options

# call init() somewhere early
def test_timeout():
  assert not options('seer').get('autofix.enabled')
  with override_options('seer', {'autofix.enabled': True}):
    assert options('seer').get('autofix.enabled')
```

### Rust

A little bit different with Rust, pass in a static array of `(namespace, option, value)` tuples.

```rust
use sentry_options::testing::override_options;
use sentry_options::{options};

// call init() somewhere early
#[test]
fn test_timeout() {
  assert_eq!(options("seer").get("autofix.enabled").unwrap(), false);
  // will be `true` for the lifetime of this guard
  let _guard = override_options(&[("seer", "autofix.enabled", true.into())]).unwrap();
  assert_eq!(options("seer").get("autofix.enabled").unwrap(), true);
}
```

## Setting an option value locally

In production, `sentry-options` reads from a Kubernetes ConfigMap mounted into a pod. To our library, this ConfigMap just looks like a normal file in our filesystem. Thus, testing locally is as easy as creating a json file and setting values there:

In `sentry-options/values/{namespace}/values.json`:

```json
{
  "options": {
    "inference.timeout": 30,
    "autofix.enabled": true
  }
}
```

These values will refresh every 5 seconds just like in production.

## Adding an option

In `sentry-options/schemas/{namespace}/schema.json`, add your option under `properties`:

```json
"new.option": {
  "type": "integer",
  "default": 42,
  "description": "configures some cool stuff"
}
```

Our library supports the following primitive types:

- `boolean`
- `integer`
- `number` (float)
- `string`

We also support *homogeneous* arrays of primitive types. In this case you need to include a required `items` property with the primitive type of the item.

```json
"array-option": {
  "type": "array",
  "default": [1, 2, 3],
  "items": {"type": "integer"},             // don't forget this!
  "description": "An example array option"
},
```

We also support objects, in two flavors, either a **struct** with named fields under `properties`, or a **dictionary** with a single value type under `additionalProperties`.

```json
"struct-option": {
  "type": "object",
  "properties": {
    "host": {"type": "string"},
    "port": {"type": "integer"},
    "label": {"type": "string", "optional": true}   // add "optional" to allow omission
  },
  "default": {"host": "localhost", "port": 8080},
  "description": "An example struct"
},
"dict-option": {
  "type": "object",
  "additionalProperties": {"type": "string"},        // every value must be a string
  "default": {},
  "description": "An example string dictionary"
}
```

A struct field can be a primitive or a primitive dictionary — **not** an array or another struct. Dicts can have arbitrary keys that match the value type, so be careful when doing validation in your code. The following are all valid `dict-option`s:

```json
  {}                               // empty is fine
  {"hello": "world"}               // one key
  {"anything": "x", "goes": "y"}   // arbitrary keys, all string values
```

Adding an option with the Python library installed should trigger the ([precommit hook](./setup.md#2-add-workflows)).

## Setting an option value

This is done in the [sentry-options-automator](https://github.com/getsentry/sentry-options-automator) repo.
Options are set in `./option-values/{namespace}/{target}/values.yaml`.

`{namespace}` is the namespace of your options, either your repo name (e.g. `seer`, `snuba`) or prefixed by your repo name (e.g. `seer-review`)

`{target}` is the location it will be deployed to.

Every namespace has one mandatory `default` target and this is where you should put most of your value definitions. Namespaces also have region targets like `us2`, `de`, etc. These are where the options will actually get deployed to, and any definitions in these targets will **override** the value set in `default`.

Take the following directory structure as an example:

```text
option-values/
├── seer/
│   ├── default/
│   │   └── values.yaml
│   ├── s4s2/
│   │   └── values.yaml
│   └── us/
│       └── values.yaml
```

```yaml
# default/values.yaml
options:
  foo: 100
```

```yaml
# s4s2/values.yaml
options: {}
```

```yaml
# us/values.yaml
options:
  foo: 200
```

Values will be taken from `option-values/seer/default` and deployed in 2 regions: `s4s2`, `us`. `s4s2` will get `foo: 100` because it has no overrides. `us` will get `foo: 200` because it overrides it in the `us` target. No options will be deployed to any other target, such as `de`, `us2`, or any of the single tenants.

We recommend creating empty (`options: {}`) `{target}/values.yaml` files in all the regions you want options deployed to, then just editing the values in the `default` target. Only edit the region specific ones if you want per-region overrides.

## Reading a feature flag

The value of a feature flag, unlike options, is evaluated based on some sort of context. For example, a feature flag is set to `true` when certain conditions regarding rollout, org, or project are `true`. After passing in and populating a `FeatureContext`, you can call `has()` and branch off of it. This is useful for targeting a subset of orgs, or rolling out to a random subset of orgs.

### Definitions

> If you have already used feature flags in Sentry before the migration, you don't need to read this. None of these mechanisms have changed.

**Feature Flag** - A flag/option that is `true` or `false` depending on how it is evaluated

**Evaluation** - The process of determining whether a flag should be `true` or `false` depending on the FeatureContext

**FeatureContext** - An object which holds context_fields and identity_fields used to describe an evaluation context.

**context_fields** - These may affect a features evaluation directly, such as only rolling out to a certain org.

**identity_fields** - This is a **subset** of context_fields. These affect a features evaluation if it has a partial rollout. We hash specifically these fields when computing whether or not an org should be included in a rollout.

### Python

```python
from sentry_options import features, FeatureContext

# don't forget init() somewhere early

context = FeatureContext(
  {"organization_id": 123, "user_email": "ken@example.org"},
  identity_fields=["organization_id"],
)

if features("seer").has("seer-explorer", context):   # don't include the "feature." prefix!
  enable_the_feature()
```

### Rust

```rust
use sentry_options::{features, FeatureContext};

// don't forget to init() somewhere early

let mut context = FeatureContext::new();
context.insert("organization_id", 123);
context.insert("user_email", "ken@example.org");
context.identity_fields(vec!["organization_id"]);

if features("seer").has("seer-explorer", &context) {   // don't include the "feature." prefix!
    enable_the_feature();
}

```

## Testing with feature flags

There are two things you might want to test:

- **Your branching logic** — build different `FeatureContext`s and assert what `has()` returns against the flag as it's defined in your local/test values (this is how the client's own tests work).
- **Force a flag on or off** — override its value with a `Feature` using the same `override_options` guard from [Testing with options](#testing-with-options). An always-on flag is a single segment with `rollout: 100` and no conditions.

**Make sure to call `init()` at some point before the guard is reached**, e.g. in a setup function.

### Python

```python
from sentry_options import features, FeatureContext
from sentry_options.testing import override_options

ALWAYS_ON = {
    "owner": {"team": "seer"},
    "created_at": "2026-01-01",
    "segments": [{"name": "all", "rollout": 100, "conditions": []}],
}

def test_explorer_enabled():
    with override_options('seer', {'feature.seer-explorer': ALWAYS_ON}):
        assert features('seer').has('seer-explorer', FeatureContext({}))
```

### Rust

The `Feature` value is a `serde_json::Value`; `json!` is the cleanest way to build one.

```rust
use sentry_options::testing::override_options;
use sentry_options::{features, FeatureContext};
use serde_json::json;

// call init() somewhere early
#[test]
fn test_explorer_enabled() {
    let _guard = override_options(&[(
        "seer",
        "feature.seer-explorer",
        json!({
            "owner": {"team": "seer"},
            "created_at": "2026-01-01",
            "segments": [{"name": "all", "rollout": 100, "conditions": []}],
        }),
    )]).unwrap();

    assert!(features("seer").has("seer-explorer", &FeatureContext::new()));
}
```

## Setting a feature flag value locally

Same as [Setting an option value locally](#setting-an-option-value-locally), drop the value in `sentry-options/values/{namespace}/values.json` — except the value is a full `Feature` object:

```json
{
  "options": {
    "feature.seer-explorer": {
      "owner": {"team": "seer"},
      "created_at": "2026-01-01",
      "segments": [
        {
          "name": "my-org",
          "rollout": 100,
          "conditions": [
            {"property": "organization_id", "operator": "in", "value": [123]}
          ]
        }
      ]
    }
  }
}
```

Check it with `features("seer").has("seer-explorer", context)` (see [Reading a feature flag](#reading-a-feature-flag)).

## Adding a feature flag

In `sentry-options/schemas/{namespace}/schema.json`, add a `feature.`-prefixed option that points at the built-in `Feature` definition. Unlike a regular option, it takes no `type` or `default`:

```json
"feature.seer-explorer": {
  "$ref": "#/definitions/Feature"
}
```

The full `Feature` shape (owner, segments, conditions, rollout) is defined in `sentry-options-validation/src/feature-schema-defs.json`.

## Setting a feature flag value

Same as [Setting an option value](#setting-an-option-value), in [sentry-options-automator](https://github.com/getsentry/sentry-options-automator) under `option-values/{namespace}/{target}/values.yaml`, with the same `default`/region target and override rules — but the value is a `Feature`:

```yaml
options:
  feature.seer-explorer:
    owner:
      team: "seer"
    created_at: "2026-01-01"
    segments:
      - name: "internal-orgs"
        rollout: 50                       # 50% of matching orgs
        conditions:
          - property: "organization_id"
            operator: "in"
            value: [123, 456]
```
