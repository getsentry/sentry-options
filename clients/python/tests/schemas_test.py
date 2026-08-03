"""E2E tests for supplying namespace schemas in memory via ``init(schemas=...)``.

Each test spins up a subprocess with its own ``SENTRY_OPTIONS_DIR`` so the Rust
``OnceLock`` starts fresh and ``init(schemas=...)`` takes effect. The schema is
passed in memory; values are still loaded from disk via the fallback chain.
"""
from __future__ import annotations

import json
from pathlib import Path

from conftest import make_options_dir
from conftest import run_isolated

_SCHEMA = json.dumps(
    {
        'version': '1.0',
        'type': 'object',
        'properties': {
            'enabled': {'type': 'boolean', 'default': False, 'description': 'Enabled'},
        },
    },
)


def _values_only_dir(tmp_path: Path) -> Path:
    """An options dir with values for ``mem-ns`` but no schema on disk."""
    options_dir = tmp_path / 'opts'
    values_dir = options_dir / 'values' / 'mem-ns'
    values_dir.mkdir(parents=True)
    values_dir.joinpath('values.json').write_text(json.dumps({'options': {'enabled': True}}))
    return options_dir


def test_init_with_in_memory_schema(tmp_path: Path) -> None:
    # No schema on disk for mem-ns; the schema comes only from init(schemas=...).
    options_dir = _values_only_dir(tmp_path)
    run_isolated(
        f"""\
        from sentry_options import init, options

        init(schemas={{"mem-ns": {_SCHEMA!r}}})
        assert options("mem-ns").get("enabled") is True
        """,
        options_dir,
    )


def test_init_with_in_memory_schema_uses_default_when_unset(tmp_path: Path) -> None:
    options_dir = tmp_path / 'opts'
    (options_dir / 'values').mkdir(parents=True)  # no values file for mem-ns
    run_isolated(
        f"""\
        from sentry_options import init, options

        init(schemas={{"mem-ns": {_SCHEMA!r}}})
        # No value set, so the schema default from the in-memory schema is served.
        assert options("mem-ns").get("enabled") is False
        """,
        options_dir,
    )


def test_init_overlays_in_memory_schema_on_disk_schemas(tmp_path: Path) -> None:
    # ``make_options_dir`` writes a ``test-ns`` schema + values to disk; the
    # in-memory ``mem-ns`` schema must be added alongside it, not replace it.
    options_dir = make_options_dir(tmp_path)
    mem_values = options_dir / 'values' / 'mem-ns'
    mem_values.mkdir(parents=True)
    mem_values.joinpath('values.json').write_text(json.dumps({'options': {'enabled': True}}))
    run_isolated(
        f"""\
        from sentry_options import init, options

        init(schemas={{"mem-ns": {_SCHEMA!r}}})
        # The on-disk namespace still resolves...
        assert options("test-ns").get("enabled") is True
        # ...and the in-memory namespace was added alongside it.
        assert options("mem-ns").get("enabled") is True
        """,
        options_dir,
    )


def test_init_in_memory_schema_conflicting_with_disk_errors(tmp_path: Path) -> None:
    # ``test-ns`` already exists on disk; overlaying it in memory must error
    # rather than silently shadow the on-disk schema.
    options_dir = make_options_dir(tmp_path)
    run_isolated(
        f"""\
        from sentry_options import init, SchemaError

        try:
            init(schemas={{"test-ns": {_SCHEMA!r}}})
        except SchemaError:
            pass
        else:
            raise AssertionError("Expected SchemaError for a namespace already on disk")
        """,
        options_dir,
    )
