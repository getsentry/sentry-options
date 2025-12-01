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
├── sentry_options/              # Python client package
│   ├── api.py                   # option_group(namespace).get(key)
│   ├── write.py                 # Python CLI for writing options
│   └── schema/                  # Python schema definitions
├── sentry-options-cli/          # Rust CLI (YAML→JSON)
├── sentry-options-validation/   # Rust validation library
└── tests/                       # Python tests
```

## Development

```bash
devenv sync                      # Setup (Rust, Python, pre-commit)
pytest tests/                    # Python tests
cargo test                       # Rust tests
pre-commit run --all-files       # Lint/format
```

## Architecture

@docs/architecture.md
