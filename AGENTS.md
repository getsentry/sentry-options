# sentry-options

Schemas and client libraries for managing Sentry configuration options. Multi-language project with Python and Rust components.

## Project Structure

```
sentry-options/
├── sentry_options/              # Python package
│   ├── api.py                   # Core API for option groups
│   ├── write.py                 # CLI to write options from YAML
│   ├── validate_type.py         # Type validation
│   ├── mypy_plugin.py           # MyPy plugin
│   └── schema/                  # Schema definitions
├── sentry-options-cli/          # Rust CLI crate
├── sentry-options-validation/   # Rust validation library
├── tests/                       # Python tests
└── .github/workflows/           # CI configuration
```

## Development Setup

This project uses [devenv](https://github.com/getsentry/devenv) and [uv](https://github.com/astral-sh/uv) for development.

```bash
devenv sync
```

This will:
- Install Rust toolchain (stable with rustfmt and clippy)
- Install uv and sync Python dependencies
- Install pre-commit hooks

## Build & Test Commands

### Python

```bash
pytest tests/                    # Run tests
tox                              # Run all test environments
mypy sentry_options/             # Type checking
pre-commit run --all-files       # Linting/formatting
```

### Rust

```bash
cargo build                      # Build
cargo test                       # Run tests
cargo fmt --all -- --check       # Check formatting
cargo clippy --workspace --all-features --tests -- -D clippy::all
```

## Code Quality

- Python: mypy (strict), flake8, autopep8, reorder-python-imports
- Rust: rustfmt, clippy
- Pre-commit hooks enforce standards
