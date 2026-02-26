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
│       ├─→ Load schemas at startup                           │
│       ├─→ Validate values against schemas                   │
│       ├─→ Background file watching + atomic refresh         │
│       └─→ Simple API: options.get(namespace, key)           │
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
│       └── sentry_options/
│           ├── __init__.py
│           └── api.py
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

| Type | JSON Schema | Example |
|------|-------------|---------|
| String | `"type": "string"` | `"hello"` |
| Integer | `"type": "integer"` | `42` |
| Float | `"type": "number"` | `3.14` |
| Boolean | `"type": "boolean"` | `true` |
| Array | `"type": "array"` | `[1,2,3]` |

> Array types of string, integer, float, and boolean are accepted.

**Future:** TypedDicts for structured options, and nested arrays.

### Schema Requirements

Each schema file must have:

- `version` - Semver string (e.g., `"1.0"`)
- `type` - Must be `"object"`
- `properties` - Map of option definitions

Each property must have:

- `type` - One of: `string`, `integer`, `number`, `boolean`, or `array`
- `default` - Default value (must match declared type)
- `description` - Human-readable description
- `items` - If the `type` is `array`. This should be an object of the following form:

`{"type": "TYPE"}` where `TYPE` is `string`, `integer`, `number`, or `boolean`.

`additionalProperties: false` is auto-injected to reject unknown options.

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
  feature.enabled: true
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

```rust
use sentry_options::{init, options};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    init()?;

    let opts = options("getsentry");
    let url = opts.get("system.url-prefix")?;
    let rate = opts.get("traces.sample-rate")?;
    Ok(())
}
```

#### Testing (Rust)

Use `ensure_initialized()` for idempotent init in tests (safe to call from parallel threads).
Use `override_options()` which returns a guard that restores values when dropped.
Overrides are validated against the schema.

```rust
use sentry_options::testing::{ensure_initialized, override_options};
use sentry_options::options;
use serde_json::json;

#[test]
fn test_feature() {
    ensure_initialized().unwrap();
    let _guard = override_options(&[
        ("getsentry", "feature.enabled", json!(true)),
    ]).unwrap();

    let opts = options("getsentry");
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
| Production | `/etc/sentry-options/{namespace}/values.json` |
| Override | `SENTRY_OPTIONS_DIR` environment variable |
| Local dev | `./sentry-options/{namespace}/values.json` |

## Open Questions

### Async Rust Support

Start with sync, add async wrapper later if needed. File reads are fast (~1ms).

### ConfigMap Sharding

How to handle namespaces exceeding 1MB? Current approach: error and require manual splitting.
