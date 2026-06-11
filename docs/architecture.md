# Architecture

## Overview

Multi-language configuration system for Sentry with JSON schemas as the source of truth.

```
┌─────────────────────────────────────────────────────────────┐
│                    Component Flow                           │
│                                                             │
│  schemas/{namespace}/schema.json (source of truth)          │
│       ↓                                                     │
│       └─→ Loaded at runtime by clients                      │
│                                                             │
│  Rust client library                                        │
│       ├─→ Load schemas at startup (disk or embedded)        │
│       ├─→ Validate values against schemas                   │
│       ├─→ Background file watching + atomic refresh         │
│       └─→ Simple API: options(namespace)?.get(key)          │
│                                                             │
│  Python client library                                      │
│       ├─→ Load schemas at startup                           │
│       ├─→ Validate values against schemas                   │
│       └─→ Simple API: option_group(namespace).get(key)      │
│                                                             │
│  Write tool (Rust CLI)                                      │
│       ├─→ Validates against schemas                         │
│       └─→ YAML → JSON conversion                            │
└─────────────────────────────────────────────────────────────┘
```

## Repository Structure

```
sentry-options/
├── Cargo.toml                       # Root Cargo workspace
├── schemas/                         # JSON schemas (source of truth)
│   ├── getsentry/
│   │   └── schema.json
│   ├── relay/
│   │   └── schema.json
│   └── sentry/
│       └── schema.json
│
├── clients/                         # All language clients
│   ├── rust/
│   │   └── sentry-options/          # Rust client library
│   │       ├── Cargo.toml
│   │       └── src/lib.rs
│   │
│   └── python/
│       ├── sentry_options/
│       │   ├── __init__.py
│       │   └── testing.py
│       ├── src/                     # Pyo3 extension
│       └── tests/                   # tests for the python client
│
├── sentry-options-cli/              # CLI tool
│   ├── Cargo.toml
│   └── src/main.rs
│
├── sentry-options-validation/       # Shared validation library (Rust)
│   ├── Cargo.toml
│   └── src/lib.rs
│
└── .github/workflows/               # CI/CD
```

## Schema Format

JSON schemas define available options with types and defaults:

```json
{
  "version": "1.0",
  "type": "object",
  "properties": {
    "system.url-prefix": {
      "type": "string",
      "default": "https://sentry.io",
      "description": "Base URL for the Sentry instance"
    },
    "traces.sample-rate": {
      "type": "number",
      "default": 0.0,
      "description": "Sample rate for traces (0.0-1.0)"
    }
  }
}
```

### Supported Types

| Type    | JSON Schema                       | Example                               |
|---------|-----------------------------------|---------------------------------------|
| String  | `"type": "string"`                | `"hello"`                             |
| Integer | `"type": "integer"`               | `42`                                  |
| Float   | `"type": "number"`                | `3.14`                                |
| Boolean | `"type": "boolean"`               | `true`                                |
| Array   | `"type": "array"`                 | `[1,2,3]`                             |
| Object  | `"type": "object"`                | `{"host": "localhost", "port": 8080}` |
| Feature | `"$ref": "#/definitions/Feature"` |                                       |

> Array items can be primitives (`string`, `integer`, `number`, `boolean`) or `object`.

#### Object Type

Objects require a `properties` field that defines the shape. Each field in `properties`
must have a `type` (one of: `string`, `integer`, `number`, `boolean`). By default all
fields are required — partial objects are not allowed unless fields are marked optional.

```json
{
  "my-config": {
    "type": "object",
    "properties": {
      "host": { "type": "string" },
      "port": { "type": "integer" }
    },
    "default": { "host": "localhost", "port": 8080 },
    "description": "Service configuration"
  }
}
```

#### Optional Fields

Individual fields within an object (or array-of-objects item) can be marked as optional
by adding `"optional": true`. Optional fields may be omitted from defaults and values.
When omitted, no default is auto-populated — callers must handle the absence.

```json
{
  "my-config": {
    "type": "object",
    "properties": {
      "host": { "type": "string" },
      "port": { "type": "integer" },
      "label": { "type": "string", "optional": true }
    },
    "default": { "host": "localhost", "port": 8080 },
    "description": "Service configuration with optional label"
  }
}
```

In this example, `host` and `port` are required, while `label` can be omitted.
If provided, `label` must still be a `string` — type validation still applies.

Schema evolution treats any change to the `properties` shape (including adding or
removing `"optional"`) as a breaking change (`ShapeChanged`).

#### Array of Objects

Arrays of objects define the item shape in `items.properties`. All items must
follow the same shape (homogeneous).

```json
{
  "endpoints": {
    "type": "array",
    "items": {
      "type": "object",
      "properties": {
        "url": { "type": "string" },
        "weight": { "type": "integer" }
      }
    },
    "default": [],
    "description": "Weighted endpoints"
  }
}
```

Nested objects (objects within objects) are not supported — object fields must be primitives.

### Schema Requirements

Each schema file must have:

- `version` - Semver string (e.g., `"1.0"`)
- `type` - Must be `"object"`
- `properties` - Map of option definitions

Each property must have:

- `type` - One of: `string`, `integer`, `number`, `boolean`, `array`, `object`,
  or is a feature flag.
- `default` - Default value (must match declared type)
- `description` - Human-readable description
- `items` - Required when `type` is `array`. An object with `{"type": "TYPE"}` where `TYPE` is `string`, `integer`, `number`, `boolean`, or `object`. When items type is `object`, a `properties` field defining the item shape is also required.
- `properties` - Required when `type` is `object`. An object mapping field names to `{"type": "TYPE"}` where `TYPE` is `string`, `integer`, `number`, or `boolean`. Fields may include `"optional": true` to allow omission.

`additionalProperties: false` is auto-injected to reject unknown options and unknown object fields.

In schema files, feature flags should have their option names begin with `feature.` and point to an object reference:

```json
{
  "version": "0.1",
  "type": "object",
  "properties": {
    "feature.organizations:red-bar": {
        "$ref": "#/definitions/Feature"
    }
  }
}
```

The `Feature` definition is spliced into the namespace schema during `init()`. The schema
for features can be found in `sentry-options-validation/src/feature-schema-defs.json`.

## Values Format

### Input: YAML Files

Configuration values are written as YAML files organized by namespace and target:

```
configs/
├── getsentry/
│   ├── default/           # Base values (required)
│   │   ├── core.yaml
│   │   └── features.yaml
│   └── s4s/               # Target-specific overrides
│       └── overrides.yaml
└── relay/
    └── default/
        └── settings.yaml
```

Each YAML file has a single `options` key:

```yaml
options:
  system.url-prefix: "https://custom.sentry.io"
  traces.sample-rate: 0.5
  webhook.delivery-enabled: true
  feature.organizations:red-bar:
      enabled: true
      owner:
        - team: "test-team"
      created_at: "2024-01-01"
      segments:
        - name: "org-segment"
          rollout: 100
          conditions:
            - property: "organization_id"
              operator: "in"
              value: [123, 456]
```

### Target Override System

- Every namespace requires a `default` target
- Non-default targets (e.g., `s4s`, `production`) inherit from `default`
- Target-specific values override defaults

### Output: JSON Files

The write tool generates merged JSON files per namespace/target:

```
dist/
├── sentry-options-getsentry-default.json
├── sentry-options-getsentry-s4s.json
└── sentry-options-relay-default.json
```

Output format:
```json
{
  "options": {
    "system.url-prefix": "https://custom.sentry.io",
    "traces.sample-rate": 0.5
  }
}
```

## Usage

### Python

```python
from sentry_options import init, options

init()
opts = options('getsentry')
url: str = opts.get('system.url-prefix')
rate: float = opts.get('traces.sample-rate')
```

#### Typing

The package ships with a `py.typed` marker and type stubs, so mypy, pyright,
and ruff work out of the box — no `# type: ignore` needed.

`OptionValue` is exported for annotating your own code:

```python
from sentry_options import OptionValue

def process(value: OptionValue) -> None: ...
```

`OptionValue` is the union of all types an option can return:

| Schema type | Python type |
|-------------|-------------|
| `string` | `str` |
| `integer` | `int` |
| `number` | `float` |
| `boolean` | `bool` |
| `object` | `dict[str, str \| int \| float \| bool]` |
| `array` | `list[str \| int \| float \| bool \| dict[...]]` |

#### Feature flags

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

#### Testing (Python)

Use the `override_options` context manager to temporarily replace option values in tests.
Overrides are validated against the schema (unknown keys and type mismatches raise errors).
Requires `init()` to have been called first — use a `conftest.py` fixture:

```python
# conftest.py
import pytest
from sentry_options import init

@pytest.fixture(scope='session', autouse=True)
def _init_options() -> None:
    init()
```

```python
from sentry_options import options
from sentry_options.testing import override_options

def test_feature():
    with override_options('getsentry', {'feature.enabled': True}):
        assert options('getsentry').get('feature.enabled') is True

# Nesting is supported — inner overrides restore to outer values
def test_nested():
    with override_options('getsentry', {'rate': 0.5}):
        with override_options('getsentry', {'rate': 1.0}):
            assert options('getsentry').get('rate') == 1.0
        assert options('getsentry').get('rate') == 0.5
```

### Rust

Use `init_with_schemas` to embed schemas in the binary at compile time via
`include_str!`. Values still load from disk and hot-reload.

```rust
use sentry_options::{init_with_schemas, options};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_with_schemas(&[
        ("getsentry", include_str!("sentry-options/schemas/getsentry/schema.json")),
    ])?;

    let opts = options("getsentry")?;
    let url = opts.get("system.url-prefix")?;
    let rate = opts.get("traces.sample-rate")?;
    Ok(())
}
```

For local development where schemas are on disk, `init()` can be used instead:

```rust
init()?; // loads schemas and values from disk
```

#### Feature flags (Rust)

```rust
use sentry_options::{init_with_schemas, features, FeatureContext};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_with_schemas(&[
        ("getsentry", include_str!("sentry-options/schemas/getsentry/schema.json")),
    ])?;
    let mut context = FeatureContext::new();
    context.insert("user_id", 123);
    context.insert("org_id", 456);
    context.insert("user_email", "sal@example.org");
    context.identity_fields(vec!["user_id"]);

    let feature_checker = features("getsentry");
    if feature_checker.has("organizations:red-bar", &context) {
        // User has feature.
    }
    Ok(())
}
```

#### Testing (Rust)

Use `override_options()` which returns a guard that restores values when dropped.
Overrides are validated against the schema.

```rust
use sentry_options::testing::override_options;
use sentry_options::{init, options};
use serde_json::json;

#[test]
fn test_feature() {
    init().unwrap();
    let _guard = override_options(&[
        ("getsentry", "feature.enabled", json!(true)),
    ]).unwrap();

    let opts = options("getsentry").unwrap();
    assert_eq!(opts.get("feature.enabled").unwrap(), json!(true));
    // guard dropped here — value restored
}
```

Overrides are thread-local and won't apply to spawned threads.

### Write Tool

```bash
sentry-options-cli --root configs/ --schemas schemas/ --out dist/
```

## Design Decisions

### 1. Runtime Validation (No Codegen)

Schemas loaded at runtime, values validated against them.

**Rationale:**
- No build-time dependencies
- Can update schemas without rebuilding clients
- Simple to understand and debug

### 2. File Watching Strategy

Background thread with interval polling (not inotify/FSEvents).

**Rationale:**
- Works reliably with Kubernetes ConfigMaps, NFS, virtual filesystems
- Simple to implement and debug

### 3. Strict Validation

Reject unknown options, fail fast on type mismatches.

**Behavior:**
- Unknown keys in values file → Error (catches typos)
- Type mismatch → Error
- `null` values → Error (use defaults instead)

### 4. JSON Number Handling

- Schema says `integer` → Reject `5.5`, accept `5`
- Schema says `number` → Accept both `5` and `5.5`

## Config Paths

| Environment | Path |
|-------------|------|
| Production | `/etc/sentry-options/values/{namespace}/values.json` |
| Override | `SENTRY_OPTIONS_DIR` environment variable |
| Local dev | `./sentry-options/values/{namespace}/values.json` |

## Benchmarks

Rust:
```bash
cargo bench --package sentry-options
```

Python:
```bash
python clients/python/benches/bench_get.py
python clients/python/benches/bench_has.py
python clients/python/benches/bench_isset.py
```

## Open Questions

### Async Rust Support

Start with sync, add async wrapper later if needed. File reads are fast (~1ms).

### ConfigMap Sharding

How to handle namespaces exceeding 1MB? Current approach: error and require manual splitting.
