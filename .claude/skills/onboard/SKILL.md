---
name: onboard
description: "Onboard a service to sentry-options. Use when integrating a new service, creating schemas, or setting up the multi-repo sentry-options workflow."
---

# sentry-options Onboarding

This skill guides you through integrating a service with sentry-options, the file-based configuration system for Sentry services.

**Full documentation:** https://github.com/getsentry/sentry-options/blob/main/docs/integration-guide.md

## What is sentry-options?

sentry-options replaces database-stored configuration with git-managed, schema-validated config files. Services read options directly from mounted files with automatic hot-reload when values change.

**Key benefits:**
- Fast reads - Options loaded in memory
- Schema validation - Type-safe options with defaults
- Hot-reload - Values update without pod restart (~1-2 min propagation)
- Audit trail - All changes tracked in git

## Onboarding Workflow

The integration spans 3 repos with dependencies between phases:

```
Phase 1: Service Repo (e.g., seer)
    └── Create schema, CI workflow, Dockerfile changes, add dependency
              ↓ (merge and get commit SHA)
Phase 2: sentry-options-automator
    └── Register service in repos.json, create values files
              ↓ (merge)
Phase 3: ops Repo
    └── Add injection annotations to deployment.yaml
```

---

## Step 1: Gather Information

Before generating files, ask the user for:

1. **Repository name** - e.g., `seer`, `relay`
2. **Namespace** - Usually same as repo, or `{repo}-{subproject}` (e.g., `seer-autofix`)
3. **Options to configure** - For each option:
   - Name (e.g., `feature.enabled`)
   - Type: `string`, `integer`, `number`, `boolean`, `array`, or `object`
   - Default value
   - Description

**Namespace validation:** The namespace directory must be either `{repo}` (exact match) or `{repo}-*` (prefixed with repo name). For example, in the `seer` repo: `seer`, `seer-autofix`, `seer-grouping` are valid; `autofix` alone is **not valid**. Warn if the namespace doesn't follow this pattern.

---

## Phase 1: Service Repo Changes

All Phase 1 changes can be deployed together - the library uses schema defaults when values don't exist.

### 1.1 Create Schema

Generate `sentry-options/schemas/{namespace}/schema.json`:

```json
{
  "version": "1.0",
  "type": "object",
  "properties": {
    "{option_name}": {
      "type": "{option_type}",
      "default": {option_default},
      "description": "{option_description}"
    }
  }
}
```

**Template variables:**
- `{namespace}` - The namespace provided by user
- `{option_name}` - Option key (e.g., `feature.enabled`)
- `{option_type}` - One of: `string`, `integer`, `number`, `boolean`, `array`, `object`
- `{option_default}` - Default value (must match type, strings need quotes)
- `{option_description}` - Human-readable description

**Supported types:** `string`, `integer`, `number`, `boolean`, `array`, `object`

**For array types:**
- Include an `items` property specifying the element type: `{"type": "string"}`, `{"type": "integer"}`, `{"type": "number"}`, `{"type": "boolean"}`, or `{"type": "object", "properties": {...}}`
- Default must be an array of the specified type (e.g., `[1, 2, 3]` for integer arrays)

**Example array option:**
```json
"feature.allowed_ids": {
  "type": "array",
  "items": {"type": "integer"},
  "default": [1, 2, 3],
  "description": "List of allowed IDs"
}
```

**For object types:**
- Include a `properties` field defining the shape. Each field has a `type` (primitives only: `string`, `integer`, `number`, `boolean`)
- Fields are required by default. Add `"optional": true` to allow omission
- Nested objects are not supported — field values must be primitives

**Example object option:**
```json
"database.config": {
  "type": "object",
  "properties": {
    "host": {"type": "string"},
    "port": {"type": "integer"},
    "label": {"type": "string", "optional": true}
  },
  "default": {"host": "localhost", "port": 8080},
  "description": "Database configuration"
}
```

**Example array of objects:**
```json
"service.endpoints": {
  "type": "array",
  "items": {
    "type": "object",
    "properties": {
      "url": {"type": "string"},
      "weight": {"type": "integer"}
    }
  },
  "default": [],
  "description": "Weighted service endpoints"
}
```

### 1.2 Add CI Workflow

Generate `.github/workflows/validate-sentry-options.yml`:

```yaml
name: Validate sentry-options schema

on:
  pull_request:
    paths:
      - 'sentry-options/schemas/**'

jobs:
  validate:
    uses: getsentry/sentry-options/.github/workflows/validate-schema.yml@1.0.2
    secrets: inherit
    with:
      schemas-path: sentry-options/schemas
```

**Note:** The workflow version `1.0.2` is current as of this writing. Instruct users to check https://github.com/getsentry/sentry-options/releases for the latest version.

### 1.3 Show Dockerfile Changes

Tell the user to add these lines to their Dockerfile:

```dockerfile
# Copy schemas into image (enables validation and defaults)
COPY sentry-options/schemas /etc/sentry-options/schemas

ENV SENTRY_OPTIONS_DIR=/etc/sentry-options
```

### 1.4 Add Dependency

#### Python
Tell the user to add to `pyproject.toml`:

```toml
dependencies = [
    "sentry_options>=1.0.2",
]
```

#### Rust
Tell the user to add to `Cargo.toml`:

```toml
[dependencies]
sentry-options = "1.0.2"
```

### 1.5 Show Usage Example

#### Python

```python
from sentry_options import init, options

# Initialize early in startup
init()

# Get options namespace
opts = options('{namespace}')

# Primitives (returns str | int | float | bool)
if opts.get('feature.enabled'):
    rate = opts.get('feature.rate_limit')

# Arrays (returns list)
ids = opts.get('feature.allowed_ids')  # e.g., [1, 2, 3]

# Objects (returns dict[str, str | int | float | bool])
config = opts.get('database.config')
host = config['host']
```

**Typing:** The package ships with `py.typed` and type stubs — mypy/pyright/ruff work out of the box. `OptionValue` is exported for type annotations:

```python
from sentry_options import OptionValue

def process(value: OptionValue) -> None: ...
```

#### Rust

```rust
use sentry_options::{init, options};

fn main() -> anyhow::Result<()> {
    // Initialize once at startup
    init()?;

    // Get namespace handle — .get() returns serde_json::Value
    let opts = options("{namespace}");

    // Primitives
    let enabled = opts.get("feature.enabled")?;
    let rate = opts.get("feature.rate_limit")?;

    // Arrays
    let ids = opts.get("feature.allowed_ids")?;

    // Objects
    let config = opts.get("database.config")?;

    Ok(())
}
```

**Rust error types:**
- `OptionsError::UnknownNamespace` - Namespace not found in schemas
- `OptionsError::UnknownOption` - Option key not in schema
- `OptionsError::Schema` - Schema parsing/validation error

### 1.6 Testing with Overrides

Both clients provide an override mechanism for tests. Overrides are validated against the schema — unknown keys and type mismatches raise errors.

#### Python Testing

Use the `override_options` context manager. Requires `init()` to have been called first — use a `conftest.py` fixture:

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
    with override_options('{namespace}', {'feature.enabled': True}):
        assert options('{namespace}').get('feature.enabled') is True

# Nesting is supported — inner overrides restore to outer values
def test_nested():
    with override_options('{namespace}', {'rate': 0.5}):
        with override_options('{namespace}', {'rate': 1.0}):
            assert options('{namespace}').get('rate') == 1.0
        assert options('{namespace}').get('rate') == 0.5
```

#### Rust Testing

`init()` is idempotent and safe to call from parallel threads. `override_options()` returns a guard that restores values when dropped.

```rust
use sentry_options::testing::override_options;
use sentry_options::{init, options};
use serde_json::json;

#[test]
fn test_feature() {
    init().unwrap();
    let _guard = override_options(&[
        ("{namespace}", "feature.enabled", json!(true)),
    ]).unwrap();

    let opts = options("{namespace}");
    assert_eq!(opts.get("feature.enabled").unwrap(), json!(true));
    // guard dropped here — value restored
}
```

Overrides are thread-local and won't affect other threads.

### 1.7 Local Testing

Before deploying, test the integration locally. The library automatically looks for a `sentry-options/` directory in the working directory.

Create a local values file (note: local testing uses JSON, not YAML):

```bash
mkdir -p sentry-options/values/{namespace}
cat > sentry-options/values/{namespace}/values.json << 'EOF'
{
  "options": {
    "feature.enabled": true,
    "feature.rate_limit": 200
  }
}
EOF
```

Directory structure should be:
```
sentry-options/
├── schemas/{namespace}/schema.json   # Schema (created in step 1.1)
└── values/{namespace}/values.json    # Test values (JSON format)
```

#### Python Testing

```python
python -c "
from sentry_options import init, options
init()
opts = options('{namespace}')
print('feature.enabled:', opts.get('feature.enabled'))
"
```

#### Rust Testing

```rust
use sentry_options::{init, options};

fn main() -> anyhow::Result<()> {
    init()?;
    let opts = options("{namespace}");
    let enabled = opts.get("feature.enabled")?;
    println!("feature.enabled: {}", enabled);
    Ok(())
}
```

To test hot-reload, modify `values.json` while the service is running - changes are picked up within 5 seconds.

### 1.8 Phase 1 Checkpoint

After generating the files, tell the user:

> **Next steps:**
> 1. Create a PR with these changes in your service repo
> 2. Merge the PR
> 3. Copy the **merge commit SHA** - you'll need it for Phase 2
>
> When ready, provide the merge commit SHA to continue.

---

## Phase 2: sentry-options-automator Changes

**Dependency:** The schema must exist in the service repo first (CI fetches it for validation).

### 2.1 Generate repos.json Entry

Tell the user to add this entry to `repos.json` in sentry-options-automator:

```json
{
  "repos": {
    "{repo}": {
      "url": "https://github.com/getsentry/{repo}",
      "path": "sentry-options/",
      "sha": "{merge_commit_sha}"
    }
  }
}
```

**Template variables:**
- `{repo}` - Repository name
- `{merge_commit_sha}` - The merge commit SHA from Phase 1

**Note:** If the repo already exists in repos.json, just update the SHA.

### 2.2 Generate Values Files

Generate `option-values/{namespace}/default/values.yaml` with the base values:

```yaml
options:
  {option_name}: {option_value}
```

**Important:** The `default/` directory contains base values that are inherited by all targets/regions. Each target that should receive a ConfigMap also needs its own directory with a `values.yaml` file. Targets with no overrides should use an empty options map:

```yaml
options: {}
```

Create a target directory for each deployment region. The current regions are: `control-silo`, `de`, `disney`, `geico`, `goldmansachs`, `ly`, `s4s`, `s4s2`, `us`. Region-specific overrides go in `option-values/{namespace}/{region}/values.yaml`.

Example structure:
```
option-values/{namespace}/
├── default/values.yaml         # Base values (inherited by all targets)
├── control-silo/values.yaml    # Empty or with overrides
├── de/values.yaml
├── disney/values.yaml
├── geico/values.yaml
├── goldmansachs/values.yaml
├── ly/values.yaml
├── s4s/values.yaml
├── s4s2/values.yaml
└── us/values.yaml
```

### 2.3 Phase 2 Checkpoint

After generating the files, tell the user:

> **Next steps:**
> 1. Create a PR in sentry-options-automator with:
>    - The repos.json entry (or SHA update)
>    - The `option-values/{namespace}/default/values.yaml` file
>    - A `values.yaml` for each target region
> 2. CI will validate your values against the schema
> 3. Merge the PR - CD will deploy ConfigMaps to all regions
>
> When ready, continue to Phase 3 for the ops repo annotation changes.

---

## Phase 3: ops Repo Changes

This phase can happen anytime — pods start normally without the ConfigMap, falling back to schema defaults.

### 3.1 Show Deployment Changes

Tell the user to add pod annotations to their `deployment.yaml` in the ops repo so the sentry-options injector automatically mounts the ConfigMap:

```yaml
# deployment.yaml
spec:
  template:
    metadata:
      annotations:
        options.sentry.io/inject: 'true'
        options.sentry.io/namespace: {namespace}
```

**Template variables:**
- `{namespace}` - The actual namespace (e.g., `seer`). For multiple namespaces, use a comma-separated list (e.g., `seer-code-review,seer`).

The injector automatically adds the necessary volumes and volume mounts based on these annotations. No manual volume configuration is needed.

---

## Schema Evolution Rules

When updating schemas, these rules are enforced by CI:

| Change | Allowed |
|--------|---------|
| Add new options | Yes |
| Add new namespaces | Yes |
| Remove options | No (coming soon) |
| Remove namespaces | No |
| Change option types | No |
| Change default values | No |

Breaking changes require a migration strategy (contact DevInfra).

---

## Updating Schemas Later

When the user updates their schema in the service repo:

1. Merge schema change to service repo
2. Get the merge commit SHA
3. Update `repos.json` in sentry-options-automator with new SHA
4. If values need to change, update them in the same PR

---

## What NOT to Put in sentry-options

Keep these as environment variables or secrets:
- Database URLs, API keys, credentials
- Infrastructure config (PORT, worker counts)
- Sentry DSN

sentry-options is for **feature flags and tunable parameters**, not secrets.

---

## Hot-Reload Behavior

- **Polling interval:** 5 seconds
- **ConfigMap propagation:** ~1-2 minutes (kubelet sync period)
- **Total latency:** ConfigMap update → ~1-2 min → file update → ~5 sec → reload

No pod restart required when values change.
