"""Tests for standalone schema validation without a runtime values store."""
from __future__ import annotations

from pathlib import Path

import pytest
from conftest import make_options_dir
from sentry_options import SchemaError
from sentry_options import SchemaRegistry
from sentry_options import UnknownNamespaceError
from sentry_options import UnknownOptionError


def test_schema_registry_validates_one_option_from_a_schema_snapshot(
    tmp_path: Path,
) -> None:
    options_dir = make_options_dir(tmp_path)

    registry = SchemaRegistry.from_directory(options_dir / 'schemas')

    registry.validate_option('test-ns', 'enabled', False)


def test_schema_registry_distinguishes_unknown_namespace_and_option(
    tmp_path: Path,
) -> None:
    options_dir = make_options_dir(tmp_path)
    registry = SchemaRegistry.from_directory(options_dir / 'schemas')

    with pytest.raises(UnknownNamespaceError, match='Unknown namespace: missing'):
        registry.validate_option('missing', 'enabled', False)
    with pytest.raises(UnknownOptionError, match="Unknown option 'missing'"):
        registry.validate_option('test-ns', 'missing', False)


def test_schema_registry_rejects_an_invalid_value_before_any_runtime_store(
    tmp_path: Path,
) -> None:
    options_dir = make_options_dir(tmp_path)
    registry = SchemaRegistry.from_directory(options_dir / 'schemas')

    with pytest.raises(SchemaError, match='not of type'):
        registry.validate_option('test-ns', 'enabled', 'false')
