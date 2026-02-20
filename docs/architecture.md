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
│       ├─→ Simple API: options(namespace).get(key)           │
│       └─→ Feature API: features(namespace).has(name, ctx)   │
│                                                             │
│  Python client library (PyO3 bindings)                      │
│       ├─→ Load schemas at startup                           │
│       ├─→ Validate values against schemas                   │
│       ├─→ Simple API: options(namespace).get(key)           │
│       └─→ Feature API: features(namespace).has(name, ctx)   │
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
│       └── src/
│           └── lib.rs               # PyO3 bindings (exposes Python API)
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

## Feature Flags

Feature flags are stored as JSON-serialized strings under `features.{name}` keys in the schema and values. The clients parse and evaluate them at runtime via `FeatureChecker`.

### Feature Config Format

Feature configs are stored as a JSON string (the schema type is `string`):

```json
{
  "enabled": true,
  "segments": [
    {
      "name": "internal-orgs",
      "rollout": 100,
      "conditions": [
        {
          "property": "organization_slug",
          "operator": {
            "kind": "in",
            "value": ["sentry", "sentry-test"]
          }
        }
      ]
    }
  ]
}
```

**Evaluation logic:**
- Feature is disabled immediately if `enabled: false`
- Segments are evaluated in order; the first matching segment wins (OR logic)
- All conditions within a segment must match (AND logic)
- `rollout` (0–100) gates the matched segment by a stable hash of the context identity

### FeatureContext

`FeatureContext` holds arbitrary application data passed to feature evaluation. Supported value types mirror the schema types: `string`, `integer`, `float`, `boolean`, and typed lists thereof.

**Identity fields** determine which context properties are hashed to compute the stable rollout bucket (0–99). The hash uses SHA1 on the colon-joined field values — matching Python's `flagpole` library for cross-language consistency. If no identity fields are set, all context fields are used (sorted lexicographically).

### Supported Operators

| Operator | Property type | Operator value |
|----------|--------------|----------------|
| `in` | scalar | array — property must appear in list |
| `not_in` | scalar | array — property must not appear in list |
| `contains` | list | scalar — list must contain the value |
| `not_contains` | list | scalar — list must not contain the value |
| `equals` | scalar | scalar — exact match (strings: case-insensitive) |
| `not_equals` | scalar | scalar — not equal |

### Schema Registration

Feature keys must be declared in the schema with `"type": "string"` and an empty-string default:

```json
{
  "features.organizations:my-feature": {
    "type": "string",
    "default": "",
    "description": "Feature flag for my-feature"
  }
}
```

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
from sentry_options import init, options, features, FeatureContext

# Initialize once at startup (reads SENTRY_OPTIONS_DIR or /etc/sentry-options)
init()

# Read configuration values
opts = options('getsentry')
url: str = opts.get('system.url-prefix')
rate: float = opts.get('traces.sample-rate')

# Evaluate feature flags
fc = features('getsentry')
ctx = FeatureContext(
    {'organization_slug': 'sentry', 'user_id': 42},
    identity_fields=['organization_slug'],
)
if fc.has('organizations:my-feature', ctx):
    ...
```

### Rust

```rust
use sentry_options::{init, options, features, FeatureContext};

fn main() -> anyhow::Result<()> {
    // Initialize once at startup (reads SENTRY_OPTIONS_DIR or /etc/sentry-options)
    init()?;

    // Read configuration values
    let opts = options("getsentry");
    let url = opts.get("system.url-prefix")?;
    let rate = opts.get("traces.sample-rate")?;

    // Evaluate feature flags
    let fc = features("getsentry");
    let mut ctx = FeatureContext::new();
    ctx.insert("organization_slug", "sentry".into());
    ctx.identity_fields(vec!["organization_slug"]);
    if fc.has("organizations:my-feature", &ctx) {
        // ...
    }

    Ok(())
}
```

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

### 5. JSON Number Handling

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
