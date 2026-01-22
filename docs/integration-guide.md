# sentry-options

File-based configuration system for Sentry services. Provides validated, hot-reloadable options without database overhead.

## Overview

sentry-options replaces database-stored configuration with git-managed, schema-validated config files. Services read options directly from mounted files with automatic hot-reload when values change.

**Key benefits:**
- **Fast reads** - No database queries, just file reads
- **Schema validation** - Type-safe options with defaults
- **Hot-reload** - Values update without pod restart (~1-2 min propagation)
- **Audit trail** - All changes tracked in git

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Build Time                              │
│  service repo (e.g., seer)                                      │
│    └── schemas/seer/schema.json  ──→  Baked into Docker image   │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                         CI Pipeline                             │
│  sentry-options-automator repo                                  │
│    └── option_values/seer/default/options.yaml                  │
│              ↓                                                  │
│    sentry-options-cli (validates against schema)                │
│              ↓                                                  │
│    ConfigMap applied to cluster                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                         Runtime                                 │
│  Kubernetes Pod                                                 │
│    ├── /etc/sentry-options/schemas/seer/schema.json (image)     │
│    └── /etc/sentry-options/values/seer/values.json (ConfigMap)  │
│              ↓                                                  │
│    sentry_options client library                                │
│    - Validates values against schema                            │
│    - Polls for file changes (5s interval)                       │
│    - Returns defaults when values missing                       │
└─────────────────────────────────────────────────────────────────┘
```

## Components

| Component | Location | Purpose |
|-----------|----------|---------|
| **Schemas** | Service repo (e.g., `seer/schemas/`) | Define options with types and defaults |
| **Values** | `sentry-options-automator/option_values/` | Runtime configuration values |
| **CLI** | `sentry-options-cli` | Fetches schemas, validates YAML, generates ConfigMaps |
| **Client** | `sentry_options` (Python) | Reads options at runtime with hot-reload |

## Integrating a New Service

Service teams are responsible for their service repo changes and adding entries to sentry-options-automator. The CI/CD infrastructure is pre-configured by the platform team.

### Phase 1: Service Repo Changes

All these changes can be deployed together - the library uses schema defaults when values don't exist.

#### 1. Create Schema

`schemas/{service}/schema.json`:
```json
{
  "version": "1.0",
  "type": "object",
  "properties": {
    "feature.enabled": {
      "type": "boolean",
      "default": false,
      "description": "Enable the feature"
    },
    "rate.limit": {
      "type": "integer",
      "default": 100,
      "description": "Rate limit per second"
    }
  }
}
```

**Supported types:** `string`, `integer`, `number`, `boolean`

**Namespace naming:** The namespace directory must be either `{repo}` (exact match) or `{repo}-*` (prefixed). For example, in the `seer` repo: `seer`, `seer-autofix`, `seer-grouping` are valid; `autofix` alone is not.

**Schema evolution rules:** The CI enforces these rules when you modify schemas:

| Change | Allowed |
|--------|---------|
| Add new options | ✅ |
| Add new namespaces | ✅ |
| Remove options | ❌ (coming soon) |
| Remove namespaces | ❌ |
| Change option types | ❌ |
| Change default values | ❌ |

Breaking changes require a migration strategy (contact DevInfra).

#### 2. Update Dockerfile

```dockerfile
# Copy schemas into image
COPY schemas/{service} /etc/sentry-options/schemas/{service}

ENV SENTRY_OPTIONS_DIR=/etc/sentry-options
```

#### 3. Add Dependency

```toml
# pyproject.toml
dependencies = [
    "sentry_options>=0.0.6",
]
```

#### 4. Initialize and Use

```python
from sentry_options import init, options

# Initialize early in startup
init()

# Get options namespace
opts = options('seer')

# Read values (returns schema default if ConfigMap doesn't exist)
if opts.get('feature.enabled'):
    rate = opts.get('rate.limit')
```

#### 5. Add Schema Validation CI

`.github/workflows/validate-sentry-options.yml`:
```yaml
name: Validate sentry-options schema

on:
  pull_request:
    paths:
      - 'schemas/**'

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # Required for comparing base and head commits
      - uses: getsentry/sentry-options/.github/actions/validate-schema@main
        with:
          schemas-path: schemas
```

### Phase 2: sentry-options-automator Changes

**Dependency:** Schema must exist in service repo first (CI fetches it for validation).

Service teams only need to do 2 things here. CI/CD is pre-configured by platform team.

#### 1. Register Service in repos.json

Add your service to `repos.json`:

```json
{
  "repos": {
    "seer": {
      "url": "https://github.com/getsentry/seer",
      "path": "schemas/seer/schema.json",
      "sha": "abc123def456..."
    }
  }
}
```

**Fields:**
- `url` - GitHub repo URL
- `path` - Path to schema.json within the repo
- `sha` - Commit SHA (pinned for reproducibility)

**Important:** When you update your schema, you must also update the SHA in repos.json to point to the commit containing the new schema.

#### 2. Create Values File

`option_values/{service}/default/options.yaml`:
```yaml
options:
  feature.enabled: true
  rate.limit: 200
```

Region-specific overrides in `option_values/{service}/{region}/options.yaml`.

**That's it!** The CI will automatically validate your values against your schema, and the CD pipeline will deploy ConfigMaps to all regions on merge.

### Updating Schema Workflow

When you update your schema in the service repo:

1. Merge schema change to service repo (e.g., `getsentry/seer`)
2. Get the merge commit SHA
3. Update `repos.json` in sentry-options-automator with new SHA
4. If values need to change, update them in same PR

```
Service Repo                    sentry-options-automator
───────────                    ─────────────────────────
PR: Update schema.json    →    PR: Update repos.json SHA
        ↓                              ↓
    Merge                          + Update values if needed
        ↓                              ↓
   (commit abc123)                  Merge
                                       ↓
                               CI validates values against new schema
                                       ↓
                               CD deploys new ConfigMaps
```

### Phase 3: ops Repo Changes

Can happen anytime - use `optional: true` so pods start without ConfigMap.

The ops repo uses Jinja2 templating. The region is available via `customer.sentry_region` (defined in cluster config files like `k8s/clusters/us/default.yaml`):

```jinja2
# deployment.yaml
volumeMounts:
- name: sentry-options-values
  mountPath: /etc/sentry-options/values/seer
  readOnly: true

volumes:
- name: sentry-options-values
  configMap:
    name: sentry-options-seer-{{ customer.sentry_region }}
    optional: true  # Pod starts with defaults if ConfigMap missing
```

This produces the correct ConfigMap name per cluster (e.g., `sentry-options-seer-us` in US, `sentry-options-seer-s4s` in S4S).

## ConfigMap Generation

The `default/` directory contains base values inherited by all targets. Region directories contain overrides merged with defaults. Each namespace/target produces a ConfigMap named `sentry-options-{namespace}-{target}`:

```
option_values/
├── seer/
│   ├── default/options.yaml    → Base values (inherited, not deployed directly)
│   ├── s4s/options.yaml        → ConfigMap: sentry-options-seer-s4s (default + s4s merged)
│   └── us/options.yaml         → ConfigMap: sentry-options-seer-us (default + us merged)
└── relay/
    ├── default/options.yaml    → Base values (inherited, not deployed directly)
    └── s4s/options.yaml        → ConfigMap: sentry-options-relay-s4s (default + s4s merged)
```

The CD pipeline iterates over each namespace and non-default target to generate and apply ConfigMaps.

## CLI Usage

```bash
# Fetch schemas from repos.json config
sentry-options-cli fetch-schemas --config repos.json --out schemas/

# Validate values against schema
sentry-options-cli validate-values --schemas schemas/ --root option_values/

# Generate ConfigMap for ONE namespace/target (run per combination)
sentry-options-cli write \
  --schemas schemas/ \
  --root option_values/ \
  --output-format configmap \
  --namespace seer \
  --target s4s \
  --commit-sha "$COMMIT_SHA" \
  --commit-timestamp "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Output: ConfigMap named "sentry-options-seer-s4s"
```

The CLI generates one ConfigMap per invocation by merging `default/` values with the specified target's overrides. The CD pipeline calls it once per namespace/target combination (excluding default).

## sentry-options-automator Structure

```
sentry-options-automator/
├── repos.json                     # Tracks schema sources (url, path, sha)
├── option_values/                 # Values for new system
│   └── {namespace}/
│       ├── default/
│       │   └── options.yaml
│       └── {region}/
│           └── options.yaml
├── options/                       # Legacy system (unchanged)
│   ├── default/
│   └── regions/
├── .github/workflows/
│   └── sentry-options-validate.yml
└── gocd/pipelines/
    ├── sentry-options.yaml        # Legacy pipeline
    └── sentry-options-new.yaml    # New system pipeline
```

**Note:** The composite action for schema validation (`validate-schema`) is in the `sentry-options` repo, not sentry-options-automator.

## Directory Structure (Runtime)

```
/etc/sentry-options/
├── schemas/                    # Baked into Docker image
│   └── {namespace}/
│       └── schema.json
└── values/                     # Mounted via ConfigMap
    └── {namespace}/
        └── values.json
```

## Hot-Reload Behavior

- **Polling interval:** 5 seconds
- **ConfigMap propagation:** ~1-2 minutes (kubelet sync period)
- **Total latency:** ConfigMap update → ~1-2 min → file update → ~5 sec → reload

No pod restart required when values change.

## Observability

The library emits Sentry transactions on reload with:
- `reload_duration_ms` - Time to reload values
- `generated_at` - When ConfigMap was generated
- `applied_at` - When application loaded values
- `propagation_delay_secs` - Time from generation to application

## What NOT to Put in sentry-options

Keep these as environment variables or secrets:
- Database URLs, API keys, credentials
- Infrastructure config (PORT, worker counts)
- Sentry DSN

sentry-options is for **feature flags and tunable parameters**, not secrets.

## Comparison with Legacy System

| Aspect | Legacy (getsentry) | New (sentry-options) |
|--------|-------------------|----------------------|
| Values location | `options/` in automator | `option_values/` in automator |
| Generation | `generate.py` | `sentry-options-cli` |
| Storage | ConfigMap → DB sync | ConfigMap → file mount |
| How consumed | Django reads from DB | Client reads from file |
| Automator pod | Required | Not needed |

## Development

```bash
# Setup
devenv sync

# Run tests
cargo test                    # Rust
pytest tests/                 # Python

# Lint
cargo clippy
pre-commit run --all-files
```
