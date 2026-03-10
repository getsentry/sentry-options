"""Tests for sentry_options.gen_stubs."""
from __future__ import annotations

import json
from pathlib import Path

import pytest
from sentry_options.gen_stubs import generate
from sentry_options.gen_stubs import namespace_to_class


@pytest.fixture(scope='session', autouse=True)
def _init_options() -> None:  # type: ignore[override]
    """No-op: gen_stubs tests don't need sentry_options initialized."""


@pytest.fixture
def schemas_dir(tmp_path: Path) -> Path:
    """A minimal controlled schema directory for testing."""
    schemas = {
        'my-service': {
            'str-key': 'string',
            'int-key': 'integer',
            'float-key': 'number',
            'bool-key': 'boolean',
            'array-key': 'array',
            'object-key': 'object',
        },
        'other-service': {
            'flag': 'boolean',
        },
    }
    for namespace, keys in schemas.items():
        ns_dir = tmp_path / namespace
        ns_dir.mkdir()
        (ns_dir / 'schema.json').write_text(
            json.dumps({
                'version': '1.0',
                'type': 'object',
                'properties': {
                    key: {'type': typ, 'default': None}
                    for key, typ in keys.items()
                },
            }),
        )
    return tmp_path


def test_namespace_to_class() -> None:
    assert namespace_to_class('seer') == 'NamespaceOptions_seer'
    assert namespace_to_class(
        'seer-code-review',
    ) == 'NamespaceOptions_seer_code_review'
    assert namespace_to_class(
        'my.namespace',
    ) == 'NamespaceOptions_my_namespace'


def test_generate_imports(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    assert 'from sentry_options._core import *' in output
    assert (
        'from sentry_options._core import NamespaceOptions, OptionValue'
        in output
    )


def test_generate_options_overloads(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    assert (
        'def options(namespace: Literal["my-service"])'
        ' -> NamespaceOptions_my_service'
        in output
    )
    assert (
        'def options(namespace: Literal["other-service"])'
        ' -> NamespaceOptions_other_service'
        in output
    )
    assert 'def options(namespace: str) -> NamespaceOptions' in output


def test_generate_namespace_subclasses(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    assert 'class NamespaceOptions_my_service(NamespaceOptions):' in output
    assert 'class NamespaceOptions_other_service(NamespaceOptions):' in output


def test_generate_typed_get_overloads(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    assert 'def get(self, key: Literal["str-key"]) -> str' in output
    assert 'def get(self, key: Literal["int-key"]) -> int' in output
    assert 'def get(self, key: Literal["float-key"]) -> float' in output
    assert 'def get(self, key: Literal["bool-key"]) -> bool' in output
    assert 'def get(self, key: Literal["array-key"]) -> list[object]' in output
    assert (
        'def get(self, key: Literal["object-key"]) -> dict[str, object]'
        in output
    )
    assert 'def get(self, key: str) -> OptionValue' in output


def test_generate_is_deterministic(schemas_dir: Path) -> None:
    assert generate(schemas_dir) == generate(schemas_dir)
