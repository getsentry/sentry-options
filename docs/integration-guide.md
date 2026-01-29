# sentry-options

File-based configuration system for Sentry services. Provides validated, hot-reloadable options without database overhead.

## Overview

sentry-options replaces database-stored configuration with git-managed, schema-validated config files. Services read options directly from mounted files with automatic hot-reload when values change.

**Key benefits:**
- **Fast reads** - Options loaded in memory, file reads only on init and updates
- **Schema validation** - Type-safe options with defaults
- **Hot-reload** - Values update without pod restart (~1-2 min propagation)
- **Audit trail** - All changes tracked in git

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Build Time                              │
│  service repo (e.g., seer)                                      │
│    └── sentry-options/schemas/seer/schema.json  ──→  Docker image│
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
| **Schemas** | Service repo (e.g., `seer/sentry-options/schemas/`) | Define options with types and defaults |
| **Values** | `sentry-options-automator/option_values/` | Runtime configuration values |
| **CLI** | `sentry-options-cli` | Fetches schemas, validates YAML, generates ConfigMaps |
| **Client** | `sentry_options` (Python) | Reads options at runtime with hot-reload |

## Integrating a New Service

Service teams are responsible for their service repo changes and adding entries to sentry-options-automator. The CI/CD infrastructure is pre-configured by the platform team.

### Phase 1: Service Repo Changes

All these changes can be deployed together - the library uses schema defaults when values don't exist.

#### 1. Create Schema

`sentry-options/schemas/{namespace}/schema.json`:
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
    "feature.rate_limit": {
      "type": "integer",
      "default": 100,
      "description": "Rate limit per second"
    }
  }
}
```

**Supported types:** `string`, `integer`, `number`, `boolean` (arrays and objects coming soon)

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

Schemas are baked into the Docker image so the client can validate values and provide defaults even before any ConfigMap is deployed.

```dockerfile
# Copy schemas into image (enables validation and defaults)
COPY sentry-options/schemas/{namespace} /etc/sentry-options/schemas/{namespace}

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
    rate = opts.get('feature.rate_limit')
```

#### 5. Test Locally

Before deploying, test your integration locally. The library automatically looks for a `sentry-options/` directory in your working directory.

**Remember:** Namespace directories must be prefixed with your repo name (e.g., `seer`, `seer-autofix`).

```bash
# Create values file for your namespace
mkdir -p sentry-options/values/{namespace}
cat > sentry-options/values/{namespace}/values.json << 'EOF'
{
  "options": {
    "feature.enabled": true,
    "feature.rate_limit": 200
  }
}
EOF

# Run your service or test script
python -c "
from sentry_options import init, options
init()
opts = options('{namespace}')
print('feature.enabled:', opts.get('feature.enabled'))
print('feature.rate_limit:', opts.get('feature.rate_limit'))
"
```

The directory structure should be:
```
sentry-options/
├── schemas/{namespace}/schema.json   # Your schema (already created in step 1)
└── values/{namespace}/values.json    # Test values
```

To test hot-reload, modify `values.json` while your service is running - changes should be picked up within 5 seconds.

#### 6. Add Schema Validation CI

`.github/workflows/validate-sentry-options.yml`:
```yaml
name: Validate sentry-options schema

on:
  pull_request:
    paths:
      - 'sentry-options/schemas/**'

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@08c6903cd8c0fde910a37f88322edcfb5dd907a8 # v5.0.0
        with:
          fetch-depth: 0  # Required for comparing base and head commits
      - uses: getsentry/sentry-options/.github/actions/validate-schema@sha
        with:
          schemas-path: sentry-options/schemas
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
      "path": "sentry-options/",
      "sha": "abc123def456..."
    }
  }
}
```

**Fields:**
- `url` - GitHub repo URL
- `path` - Path to schemas directory within the repo (contains `{namespace}/schema.json`)
- `sha` - Commit SHA (pinned for reproducibility)

**Important:** When you update your schema, you must also update the SHA in repos.json to point to the commit containing the new schema.

#### 2. Create Values File

`option_values/{namespace}/default/options.yaml`:
```yaml
options:
  feature.enabled: true
  feature.rate_limit: 200
```

Region-specific overrides in `option_values/{namespace}/{region}/options.yaml`.

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

The ops repo uses Jinja2 templating. Add volume and volumeMount to your deployment.yaml:

```yaml
# deployment.yaml
      containers:
        - name: {{ values.service }}
          # ... existing config ...
          volumeMounts:
          # ... existing mounts ...
          - name: sentry-options-values
            mountPath: /etc/sentry-options/values/{namespace}
            readOnly: true
      volumes:
      # ... existing volumes ...
      - name: sentry-options-values
        configMap:
          name: sentry-options-{namespace}-{{ customer.sentry_region }}
          optional: true  # Pod starts with defaults if ConfigMap missing
```

Replace `{namespace}` with your actual namespace (e.g., `seer`). The `customer.sentry_region` variable provides the region (us, de, s4s, etc.), producing ConfigMap names like `sentry-options-seer-us`.

## ConfigMap Generation

The `default/` directory contains base values inherited by all targets. Region directories contain overrides merged with defaults. Each namespace/target produces a ConfigMap named `sentry-options-{namespace}-{target}`:

```
option_values/
├── seer/
│   ├── default/options.yaml      → Base values (inherited, not deployed directly)
│   ├── us/options.yaml           → ConfigMap: sentry-options-seer-us (default + us merged)
│   └── de/options.yaml           → ConfigMap: sentry-options-seer-de (default + de merged)
└── relay/
    ├── default/options.yaml      → Base values (inherited, not deployed directly)
    └── us/options.yaml           → ConfigMap: sentry-options-relay-us (default + us merged)
```

**What is a target?** A target represents a deployment environment or region (e.g., `us`, `de`, `s4s`). Each target gets its own ConfigMap with values merged from `default/` plus target-specific overrides. The mapping of targets to Kubernetes clusters is configured in the CD pipeline.

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
  --target us \
  --commit-sha "$COMMIT_SHA" \
  --commit-timestamp "$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Output: ConfigMap named "sentry-options-seer-us"
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
