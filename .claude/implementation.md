# Implementation Plan: sentry-options Multi-Language Client

**Status:** Final
**Created:** 2025-11-03
**Architecture:** Runtime Type Configuration

---

## Executive Summary

Build a multi-language configuration system with:
- **JSON schemas** as language-agnostic source of truth
- **Runtime validation** against schemas when loading values
- **Fast reads** (<10ms) via file-based ConfigMaps
- **Git workflow** for configuration changes

**Key Decision:** Use runtime type validation (no codegen). Schemas are loaded and validated at runtime, providing flexibility without build-time dependencies.

---

## Architecture Overview

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

### Why This Architecture?

**Runtime Type Validation:**
- Schemas loaded at startup, values validated against them
- No build-time codegen required
- Flexible: can update schemas without rebuilding clients
- Simple to understand and debug

**Independent Clients:**
- Rust and Python clients are separate implementations
- Both read same schema format
- Validation logic in each language (simpler than FFI)

**Fail-Fast at Startup:**
- Invalid values cause immediate errors on load
- No silent failures or fallbacks to wrong types

---

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
│   │       └── src/
│   │           └── lib.rs           # Runtime loading + validation
│   │
│   └── python/                      # Python client (FUTURE)
│       └── sentry_options/
│           ├── __init__.py
│           └── api.py               # Runtime loading + validation
│
├── sentry-options-cli/              # CLI tool
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
│
├── sentry-options-validation/       # Shared validation library (Rust)
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
│
├── sentry_options/                  # Current Python package
│   ├── api.py
│   ├── write.py
│   └── schema/
│
├── .claude/                         # Claude Code context
│   └── implementation.md            # This file
│
└── .github/workflows/               # CI/CD
    ├── main.yml
    └── release.yml
```

### Key Structural Decisions

**Runtime Validation (No Codegen):**
- Schemas loaded at runtime, not at build time
- No build.rs, no generated macros, no .pyi stubs
- Simple function call API: `options.get(namespace, key)`

**Lazy Loading Architecture:**
- First `get()` call initializes automatically
- Background file watching with atomic refresh (never blocks reads)

**Convention-Based Config Paths:**
- Default: `/etc/sentry-options/{namespace}/values.json`
- Override: `SENTRY_OPTIONS_DIR` environment variable
- Local dev: `./sentry-options/{namespace}/values.json` (fallback)

---

## Runtime Type Validation

### How It Works

**Input: JSON Schema**
```json
// schemas/getsentry/schema.json
{
  "system.url-prefix": {
    "type": "str",
    "default": "https://sentry.io"
  },
  "traces.sample-rate": {
    "type": "float",
    "default": 0.0
  },
  "store.load-shed-projects": {
    "type": "list[int]",
    "default": []
  }
}
```

### Rust Client

**Usage:**
```rust
use sentry_options::Options;

fn main() -> anyhow::Result<()> {
    // Initialize once (loads schemas + values, starts background watcher)
    let options = Options::new(schemas_dir, values_dir)?;

    // Get values - returns serde_json::Value
    let url = options.get("getsentry", "system.url-prefix")?;
    let rate = options.get("getsentry", "traces.sample-rate")?;

    // Unknown key = runtime error
    let x = options.get("getsentry", "unknown-key")?;  // ❌ Runtime error

    Ok(())
}
```

### Python Client

**Usage:**
```python
opts = option_group('getsentry')
url: str = opts.get('system.url-prefix')
rate: float = opts.get('traces.sample-rate')
typo = opts.get('unknown-key')  # ❌ Runtime error
```

### Validation Behavior

- **Startup:** Load schemas and values, validate all values against schemas
- **Invalid values:** Fail immediately with descriptive error
- **Unknown keys in values file:** Fail immediately (catches typos)
- **Unknown key in get():** Return error

---

## JSON Schema Format

### Specification

**Primitives:**
```json
{
  "system.url-prefix": {"type": "str", "default": "https://sentry.io"},
  "max-retries": {"type": "int", "default": 3},
  "traces.sample-rate": {"type": "float", "default": 0.0},
  "symbolicator.enabled": {"type": "bool", "default": true}
}
```

**Lists:**
```json
{
  "store.load-shed-projects": {"type": "list[int]", "default": []},
  "allowed-domains": {"type": "list[str]", "default": ["sentry.io"]}
}
```

### Type Grammar

```
Type := Primitive | List
Primitive := "str" | "int" | "float" | "bool"
List := "list[" Primitive "]"
```

**Supported:** Primitives and lists of primitives
**Not supported:** Nested lists, dicts, TypedDict, unions

**Rationale:** Keep it simple for MVP, can extend later if needed.

---

## Implementation Phases

### Phase 1: Rust Client Library

**Components:**
1. **Schema system** - Load and parse JSON schemas at runtime
2. **Value loading** - Load values from JSON files
3. **Validation** - Validate values against schemas at load time
4. **Background watcher** - Poll for file changes, reload when detected
5. **Testing utilities** - Override mechanism for tests

**Key Features:**
- **Runtime validation:** Schemas loaded at startup, values validated against them
- **Simple API:** `Options::new()` and `options.get(namespace, key)`
- **Background polling:** Interval-based mtime checks (works with ConfigMaps/NFS)
- **Fail-fast startup:** Invalid values cause immediate error
- **Fail-safe runtime:** Failed reloads keep old values

**Deliverables:**
- Working Rust library with runtime validation
- Comprehensive unit and integration tests

---

### Phase 2: Write Tool

**Purpose:** CLI tool that validates YAML configs and generates JSON

**Functionality:**
- Walk directory structure
- Load schemas and validate
- Parse YAML files (serde_yaml)
- Generate JSON files

**Deliverables:**
- Working CLI: `sentry-options-cli --root configs/`
- Comprehensive error messages

---

### Phase 3: Python Client

**Purpose:** Native Python client with same behavior as Rust

**Components:**
1. **Schema loading** - Load JSON schemas
2. **Value loading** - Load values from JSON files
3. **Validation** - Validate values against schemas
4. **Testing utilities** - Override mechanism for tests

**Deliverables:**
- Working Python library
- Same validation behavior as Rust client
- Python tests

---

## Key Design Decisions

### 1. File Watching Strategy

**Approach:** Background thread with interval polling

**Implementation:**
- Background thread polls mtime of value files at configurable interval (default 5s)
- Reload values when mtime changes
- Uses interval polling instead of inotify/FSEvents

**Rationale:**
- Works reliably with Kubernetes ConfigMaps, NFS, and virtual filesystems
- `notify` crate doesn't work well with mounted volumes
- Simple to implement and debug

### 2. Schema Version Compatibility

**Approach:** Strict loading and getting

**Behavior:**
- When loading values.json: Error if unknown options present
- When calling `.get()`: Error if option not in schema

**Rationale:**
- Catches typos and misconfigurations early
- Ensures values file always matches schema
- No silent failures or ignored data

### 3. JSON Number Handling

**Approach:** Int strict, float permissive

**Behavior:**
- Schema says `"int"` → Reject `5.5`, accept `5`
- Schema says `"float"` → Accept both `5` and `5.5`

**Rationale:**
- Prevents accidental type errors
- Common pattern in typed systems

### 4. Null Handling

**Approach:** Null is an error

**Behavior:**
- Missing key → Use schema default
- Empty list `[]` → Return empty list
- `null` → Validation error

**Rationale:**
- Forces explicitness in configuration
- Avoids ambiguity

### 5. Runtime Validation

**Approach:** Always validate (can be made optional later)

**Rationale:**
- Defense in depth (catches manual ConfigMap edits)
- Catches corruption or version mismatches
- Small overhead (~1ms when reloading)

### 6. Type System Scope

**Approach:** Start with primitives + lists, extend later if needed

**Supported:**
- `str`, `int`, `float`, `bool`
- `list[str]`, `list[int]`, `list[float]`, `list[bool]`

**Not supported (yet):**
- Nested lists, dicts, TypedDict, unions

**Rationale:**
- Covers design doc requirements (load-shed-projects needs `list[int]`)
- Simple to implement and validate
- Can add complex types later without breaking changes (use structured JSON format)

---

## Success Metrics

### Performance Targets
- ✅ Option reads: < 10ms (target: <1ms cached)
- ✅ File reload: < 5ms when changed
- ✅ Memory: < 1MB per option group
- ✅ ConfigMap propagation: 1-2 minutes (acceptable)

### Validation
- ✅ 100% of config changes validated before deploy
- ✅ Type errors caught at CI time
- ✅ Schema violations rejected

### Developer Experience
- ✅ Type-safe option access in Python and Rust
- ✅ Fast local development (no DB needed)
- ✅ Easy testing (override utilities)

---

## Risk Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| ConfigMap size exceeds 1MB | Medium | Write tool enforces limit early, error with clear message |
| File watching fails on some filesystems | Medium | Use interval polling (works everywhere) |
| Schema version skew | Low | Strict validation catches mismatches at startup |
| Performance regression | Low | Benchmark in CI |

---

## Dependencies & Prerequisites

### Required Tools
- Rust toolchain (stable)
- Python 3.12+

### External Dependencies
- serde_json, serde_yaml (standard Rust crates)
- jsonschema (JSON Schema validation)

---

## Open Questions

### 1. Async Rust Support
**Question:** Provide async API for Rust?

**Decision:** Start with sync, add async wrapper later if needed
- File reads are fast (~1ms)
- Sync is simpler
- Can add `async fn get_async()` later without breaking changes

### 2. ConfigMap Sharding
**Question:** How to handle groups exceeding 1MB?

**Recommendation:** Start with error, implement auto-shard if needed.

---

## Appendix: Usage Examples

### Python Usage
```python
from sentry_options import option_group

# Read options
opts = option_group('getsentry')
url: str = opts.get('system.url-prefix')
rate: float = opts.get('traces.sample-rate')
projects: list[int] = opts.get('store.load-shed-projects')

# Testing
from sentry_options.testing import override_options

def test_feature():
    with override_options(getsentry={'feature.enabled': True}):
        assert my_feature_works()
```

### Rust Usage
```rust
use sentry_options::Options;

fn main() -> anyhow::Result<()> {
    // Initialize once (loads schemas + values, starts background watcher)
    let options = Options::new(schemas_dir, values_dir)?;

    // Get values - returns serde_json::Value
    let url = options.get("getsentry", "system.url-prefix")?;
    let rate = options.get("getsentry", "traces.sample-rate")?;
    let projects = options.get("getsentry", "store.load-shed-projects")?;

    Ok(())
}
```

### Write Tool Usage
```bash
# Validate schemas and definitions
sentry-options-cli --root /path/to/options
```

---

**END OF IMPLEMENTATION PLAN**
