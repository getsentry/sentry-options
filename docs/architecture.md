# Architecture

`sentry-options` is a file-based configuration and feature-flag platform for Sentry services. Options and feature flags are declared in a JSON **schema** (checked into the service's own repo), their **values** are set in `sentry-options-automator`, and a CLI compiles the two into a Kubernetes **ConfigMap** that is mounted into the pod and read at runtime by a Rust or Python client.

There is no database and no network call on the read path: values are validated against the schema, served from memory, and refreshed from disk on read. The schema ships inside the service image, so a pod can validate values and serve defaults even before any ConfigMap exists.

## System Overview

The system spans three repos and three stages:

```
Build time — service repo (e.g. seer)
  sentry-options/schemas/{ns}/schema.json ──COPY──▶ image (/etc/sentry-options/schemas)

CI / CD — sentry-options-automator
  option-values/{ns}/{target}/values.yaml
        │  sentry-options-cli write   (validate values vs schema, merge default + target)
        ▼
  ConfigMap  sentry-options-{ns}      ──applied once per region──▶ cluster

Runtime — Kubernetes pod
  envoy-injector mounts the ConfigMap at /etc/sentry-options/values/{ns}/values.json
        ▼
  sentry_options client  ── validate, serve defaults when missing, refresh on read ──▶ your code
```

## Components

| Component | Location | Role |
|-----------|----------|------|
| Schemas | service repo, `sentry-options/schemas/{ns}/schema.json` | Declare options & feature flags with types and defaults (source of truth) |
| Values | `sentry-options-automator`, `option-values/{ns}/{target}/values.yaml` | Per-target option values |
| `sentry-options-cli` | this repo | Validate schemas/values, merge targets, emit ConfigMaps / JSON |
| `sentry-options-validation` | this repo | Shared library: meta-schema validation, value validation, and the runtime value store the clients use |
| Clients | `clients/rust`, `clients/python` | `sentry-options` (Rust crate) and `sentry_options` (Python, PyO3 bindings over the Rust core) |
| `validate-schema` | `.github/workflows/` | Reusable workflow enforcing the evolution rules on PRs |
| `sentry-options-gen-stubs` | this repo | Pre-commit hook that generates typed Python stubs from schemas |
| `envoy-injector` | ops repo | Mutating admission webhook that mounts the ConfigMap into annotated pods |

## Schema Format

A schema is a JSON file at `sentry-options/schemas/{namespace}/schema.json`:

```json
{ "version": "1.0", "type": "object", "properties": { } }
```

- `version` — a `MAJOR.MINOR` string (meta-schema pattern `^[0-9]+\.[0-9]+$`); full three-part semver is rejected.
- `type` — always `"object"`.
- `properties` — map of option name → definition.

The meta-schema that enforces all of this is `sentry-options-validation/src/namespace-schema.json`.

### Option types

Each non-feature option requires `type`, `default`, and `description`.

**Primitives** — `string`, `integer`, `number`, `boolean`:

```json
"inference.timeout": { "type": "integer", "default": 100, "description": "Inference timeout (s)" }
```

**Arrays** — homogeneous; require an `items` type:

```json
"allowed.ids": { "type": "array", "items": {"type": "integer"}, "default": [], "description": "Allowed IDs" }
```

**Objects** come in two forms:

- **Struct** — fixed named fields under `properties`. Fields are required unless marked `"optional": true`.

  ```json
  "db.config": {
    "type": "object",
    "properties": {
      "host": {"type": "string"},
      "port": {"type": "integer"},
      "label": {"type": "string", "optional": true}
    },
    "default": {"host": "localhost", "port": 8080},
    "description": "Database config"
  }
  ```

- **Dict (map)** — arbitrary string keys, a single value type, declared with `additionalProperties`:

  ```json
  "service.tags": { "type": "object", "additionalProperties": {"type": "string"}, "default": {}, "description": "String tags" }
  ```

**Nested value types** — the value type of an array's `items`, a map's `additionalProperties`, or a struct field is recursive: it can be a primitive, an array, a map, or a struct, nested arbitrarily. So a map of structs, a map of arrays, or an array of structs are all expressible:

```json
"endpoints": {
  "type": "array",
  "items": { "type": "object", "properties": { "url": {"type": "string"}, "weight": {"type": "integer"} } },
  "default": [],
  "description": "Weighted endpoints"
},
"org.tweaks": {
  "type": "object",
  "additionalProperties": { "type": "object", "additionalProperties": {"type": "integer"} },
  "default": {},
  "description": "Per-org integer tweaks: {orgId: {setting: int}}"
}
```

### Feature flags

A feature flag is an option whose name begins with `feature.` and whose definition is a reference to the built-in `Feature` schema:

```json
"feature.red-bar": { "$ref": "#/definitions/Feature" }
```

The `Feature` definition (`sentry-options-validation/src/feature-schema-defs.json`) is spliced into the namespace schema when the schema is loaded, so feature values are validated against it. A `Feature` value:

```yaml
owner:
  team: "my-team"          # required (email optional)
created_at: "2026-01-01"   # required
enabled: true              # optional, defaults to true
segments:                  # required
  - name: "internal"
    rollout: 100           # optional, defaults to 100
    conditions:
      - { property: "organization_id", operator: "in", value: [123] }
```

Condition operators: `in`, `not_in`, `contains`, `not_contains`, `equals`, `not_equals`, `matches`, `not_matches`.

### Validation internals

When a schema is loaded, `sentry-options-validation` compiles a JSON-Schema validator from it and injects two constraints (`inject_object_constraints`):

- `additionalProperties: false` is added to the top-level options object and to every struct, so unknown keys are rejected — **except** when the schema explicitly declares `additionalProperties` (the dict form), which is left open to arbitrary keys.
- For each struct, a `required` array is derived from the fields that are *not* marked `"optional": true`.

Both constraints are injected recursively, descending into `properties`, `additionalProperties`, and `items`, so a struct nested inside a map or array is validated as strictly as a top-level one.

`integer` rejects fractional values while `number` accepts both, and each option's `default` is validated against that option's own type at load time.

## Values & Targets

Values are YAML files under `option-values/{namespace}/{target}/`, each with a single `options:` key:

```yaml
options:
  inference.timeout: 200
```

Every namespace has a mandatory `default` target (the base) plus one directory per region target (`us`, `de`, `s4s2`, …). A target's values are merged on top of `default`, with target keys overriding. A namespace is only deployed to a region that has its own target directory — `default` alone deploys nowhere.

`sentry-options-cli write` compiles one namespace/target into output in one of two formats:

- `--output-format configmap` → a `ConfigMap` named `sentry-options-{namespace}` (the target is *not* in the name; each region's cluster gets its own copy). It carries a `generated_at` value plus optional `commit_sha` / `commit_timestamp` annotations.
- `--output-format json` → a file `sentry-options-{namespace}-{target}.json` shaped `{"options": {...}, "generated_at": "..."}`.

Generated ConfigMaps are capped just under 1 MiB; the CLI errors if a namespace exceeds it. The CD pipeline runs `write` once per namespace/target and applies per region — a namespace with no directory for a region is skipped for that region.

## Runtime Model

The client resolves its base directory as: the `SENTRY_OPTIONS_DIR` env var → `/etc/sentry-options` if it exists → `./sentry-options/`. Within it:

- schemas at `{dir}/schemas/{ns}/schema.json`
- values at `{dir}/values/{ns}/values.json` (shape `{"options": {...}, "generated_at": "..."}`)

A read — `options(ns).get(key)` — validates the loaded values against the schema and returns the value, or the schema **default** when the key is absent from the values file. Unknown keys in a values file are **stripped with a warning**, not errored; this tolerates the deploy-time race where a ConfigMap is updated before the new schema ships.

### Initialization

Options must be initialized once at startup, guarded by a global `OnceLock`. Rust uses a builder, `InitBuilder`. Python folds the options into one function. In Rust, chain any subset of the `with_*` methods, then call `.init()`:

| `InitBuilder` method | Behavior |
|----------------------|----------|
| _(no overrides)_ | Resolve the directory and load schemas from disk |
| `.with_directory(path)` | Load schemas and values from a specific base directory |
| `.with_schemas(&[(ns, json)])` | Use schemas embedded in the binary via `include_str!`. values still load from disk |
| `.with_callback(cb)` | Register a reload callback |

`init()` is a shorthand for `InitBuilder::new().init()`; the standalone `init_with_schemas` / `init_with_propagation_callback` functions are deprecated in favor of the builder. Python: `init(on_propagation=None)`.

The `init()` shorthand and Python's `init()` are idempotent — calling again is a no-op. The builder's `.init()` instead returns `OptionsError::AlreadyInitialized` when options are already initialized, so re-initializing with different settings is a loud error rather than a silent no-op.

### Refresh-on-read

There is no background thread. Values are refreshed lazily, on the reading thread:

- The current snapshot lives in an `ArcSwap`, so reads are lock-free.
- On a read, if the snapshot is older than a **5-second threshold** (plus a small per-call jitter derived from the stack address, so threads don't all refresh on the same boundary), the reading thread re-stats and re-reads the files and publishes the new snapshot. Concurrent refreshers are harmless — last writer wins. On an I/O error the timestamp is still advanced to avoid hammering the disk.

End to end, a value change propagates as: ConfigMap update → ~1–2 min kubelet sync to the mounted file → ≤5 s until the next read picks it up. No pod restart is required.

### Propagation metric

When a refresh observes a newer `generated_at` than the previous snapshot, the propagation callback (`InitBuilder::with_callback` in Rust, `on_propagation` in Python) fires with the namespace and the delay between generation and load — useful for measuring deploy lag.

## Feature Flag Evaluation

`features(ns).has(name, context)` returns a `bool`. The `name` omits the `feature.` prefix — `has` prepends it (so a `feature.red-bar` schema entry is checked as `has("red-bar", ...)`).

A `FeatureContext` carries arbitrary key→value data plus an optional set of `identity_fields`. Evaluation (`clients/rust/src/features.rs`):

- A feature's `segments` are tried in order; the **first** segment whose conditions all match (logical AND) decides the result via its rollout. If no segment matches, the feature is off. A feature with `enabled: false` is always off.
- Rollout: `rollout: 0` → off, `>= 100` → on, otherwise the context is in the rollout iff `id % 100 < rollout`.
- The context `id` is a SHA-1 hash built from the identity fields: each identity field present in the data, sorted, joined as `key:value:...`. If no `identity_fields` are set, **all** data keys are used (so the id — and the rollout bucket — changes whenever any field differs).

`identity_fields` therefore decide what a percentage rollout is bucketed by. Setting them to a stable entity such as `["organization_id"]` keeps a 50% rollout consistent per-org across requests and services; leaving them unset makes bucketing depend on the entire context. This mirrors flagpole's model — context fields are for *targeting*, identity fields are for stable *bucketing*.

## Schema Evolution & Validation

The `validate-schema` reusable workflow runs on schema PRs and enforces (`sentry-options-cli/src/schema_evolution.rs`):

| Change | Allowed |
|--------|---------|
| Add an option | ✅ |
| Add a namespace | ✅ |
| Remove an option | ✅ (warns if still referenced in automator values) |
| Remove a namespace | ❌ |
| Change an option's type | ❌ |
| Change an option's default | ❌ |
| Change an object / array-of-object shape | ❌ (`ShapeChanged`) |

New namespaces must be named `{repo}` (exact) or `{repo}-*` (prefixed with `{repo}-`).

Two CLI paths feed validation: `validate-schema-changes` diffs a schema between a base and head SHA (the PR), and `fetch-schemas` pulls each service's schema from the `url`/`path` declared in the automator's `repos.json` so values can be validated against it.

## The Injector (ops)

In production the ConfigMap is mounted by `envoy-injector`, a `MutatingWebhookConfiguration` (ops repo, `k8s/services/envoy-injector/webhook.yaml`) that intercepts pod creation. A pod opts in with annotations:

```yaml
options.sentry.io/inject: 'true'
options.sentry.io/namespace: seer        # comma-separated for multiple namespaces
```

When present, the webhook patches the pod to add a volume backed by the `sentry-options-{namespace}` ConfigMap and a volumeMount at `/etc/sentry-options/values/{namespace}/`. Details worth knowing:

- The webhook only fires in Kubernetes namespaces labeled `sentry-envoy-injection: enabled` (the `default` namespace carries this label, which is where most services run).
- `failurePolicy: Fail` — if the injector is unreachable, pod creation in those namespaces fails rather than starting un-injected.
- It runs on pod `CREATE` only, so annotation changes take effect on the next deploy/restart, and the patch exists on the running pod rather than in the committed manifest.
