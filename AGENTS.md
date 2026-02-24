# sentry-options

File-based configuration system for Sentry. Replaces database-stored options with git-managed, validated configs for faster reads and auditable changes.

## Key Concepts

- **Schemas** (`schemas/{ns}/schema.json`) - Define options with types/defaults (source of truth)
- **Values** (YAML files) - Override defaults, organized by namespace and target (e.g., `default`, `s4s`)
- **CLI** - Validates YAML against schemas, merges targets, outputs JSON for deployment
- **Clients** (Python, Rust) - Load JSON values at runtime with schema validation

## Project Structure

```
sentry-options/
├── clients/
│   ├── python/                  # Python client (PyO3 bindings)
│   └── rust/sentry-options/     # Rust client library
├── sentry-options-cli/          # Rust CLI (YAML→JSON)
├── sentry-options-validation/   # Rust validation library
└── schemas/                     # JSON schemas (source of truth)
```

## Development

```bash
devenv sync                      # Setup (Rust, Python, pre-commit)
cargo test                       # Rust tests
cd clients/python && pytest tests/  # Python client tests
pre-commit run --all-files       # Lint/format
```

## Architecture

@docs/architecture.md
