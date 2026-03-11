"""Tests for sentry_options_gen_stubs."""
from __future__ import annotations

import json
from pathlib import Path

import pytest
from sentry_options_gen_stubs import generate
from sentry_options_gen_stubs import namespace_to_class


@pytest.fixture
def schemas_dir(tmp_path: Path) -> Path:
    """A minimal controlled schema directory for testing."""
    schemas = {
        'my-service': {
            'str-key': {'type': 'string'},
            'int-key': {'type': 'integer'},
            'float-key': {'type': 'number'},
            'bool-key': {'type': 'boolean'},
            # typed array with primitive items
            'int-list-key': {
                'type': 'array',
                'items': {'type': 'integer'},
            },
            # array of objects with properties
            'obj-list-key': {
                'type': 'array',
                'items': {
                    'type': 'object',
                    'properties': {
                        'url': {'type': 'string'},
                        'weight': {'type': 'integer'},
                    },
                },
            },
            # object with required and optional fields
            'typed-obj-key': {
                'type': 'object',
                'properties': {
                    'host': {'type': 'string'},
                    'port': {'type': 'integer'},
                    'label': {'type': 'string', 'optional': True},
                },
            },
        },
        'other-service': {
            'flag': {'type': 'boolean'},
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
                    key: {**prop, 'default': None}
                    for key, prop in keys.items()
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
    assert 'from sentry_options._core import *' not in output
    assert 'from sentry_options._core import (' in output
    for symbol in (
        'InitializationError',
        'NamespaceOptions',
        'OptionsError',
        'OptionValue',
        'SchemaError',
        'UnknownNamespaceError',
        'UnknownOptionError',
        'init',
    ):
        assert symbol in output
    assert 'from typing import Literal, NotRequired, TypedDict, overload' in output


def test_unknown_type_raises(tmp_path: Path) -> None:
    ns_dir = tmp_path / 'svc'
    ns_dir.mkdir()
    (ns_dir / 'schema.json').write_text(
        json.dumps({
            'version': '1.0',
            'type': 'object',
            'properties': {
                'bad-key': {'type': 'unknown-type', 'default': None},
            },
        }),
    )
    with pytest.raises(SystemExit):
        generate(tmp_path)


def test_array_without_items_raises(tmp_path: Path) -> None:
    ns_dir = tmp_path / 'svc'
    ns_dir.mkdir()
    (ns_dir / 'schema.json').write_text(
        json.dumps({
            'version': '1.0',
            'type': 'object',
            'properties': {
                'arr': {'type': 'array', 'default': []},
            },
        }),
    )
    with pytest.raises(SystemExit):
        generate(tmp_path)


def test_object_without_properties_raises(tmp_path: Path) -> None:
    ns_dir = tmp_path / 'svc'
    ns_dir.mkdir()
    (ns_dir / 'schema.json').write_text(
        json.dumps({
            'version': '1.0',
            'type': 'object',
            'properties': {
                'obj': {'type': 'object', 'default': {}},
            },
        }),
    )
    with pytest.raises(SystemExit):
        generate(tmp_path)


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
    assert (
        'class NamespaceOptions_other_service(NamespaceOptions):' in output
    )


def test_generate_primitive_overloads(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    assert 'def get(self, key: Literal["str-key"]) -> str' in output
    assert 'def get(self, key: Literal["int-key"]) -> int' in output
    assert 'def get(self, key: Literal["float-key"]) -> float' in output
    assert 'def get(self, key: Literal["bool-key"]) -> bool' in output
    assert 'def get(self, key: str) -> OptionValue' in output


def test_generate_typed_array(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    assert (
        'def get(self, key: Literal["int-list-key"]) -> list[int]'
        in output
    )


def test_generate_array_of_objects(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    td = '_NamespaceOptions_my_service_0_Item'
    assert f"{td} = TypedDict('{td}'" in output
    assert "    'url': str," in output
    assert "    'weight': int," in output
    assert (
        f'def get(self, key: Literal["obj-list-key"]) -> list[{td}]'
        in output
    )


def test_generate_optional_field_in_array_of_objects(tmp_path: Path) -> None:
    ns_dir = tmp_path / 'svc'
    ns_dir.mkdir()
    (ns_dir / 'schema.json').write_text(
        json.dumps({
            'version': '1.0',
            'type': 'object',
            'properties': {
                'endpoints': {
                    'type': 'array',
                    'items': {
                        'type': 'object',
                        'properties': {
                            'url': {'type': 'string'},
                            'label': {'type': 'string', 'optional': True},
                        },
                    },
                    'default': [],
                },
            },
        }),
    )
    output = generate(tmp_path)
    td = '_NamespaceOptions_svc_0_Item'
    assert f"{td} = TypedDict('{td}'" in output
    assert "    'url': str," in output
    assert "    'label': NotRequired[str]," in output


def test_generate_typed_object(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    td = '_NamespaceOptions_my_service_1_Dict'
    assert f"{td} = TypedDict('{td}'" in output
    assert "    'host': str," in output
    assert "    'port': int," in output
    # optional field uses NotRequired
    assert "    'label': NotRequired[str]," in output
    assert (
        f'def get(self, key: Literal["typed-obj-key"]) -> {td}' in output
    )


def test_typed_dict_field_names_preserved(tmp_path: Path) -> None:
    """Nested object keys with hyphens are preserved as-is via functional
    TypedDict form, so runtime dict keys match the type stub exactly."""

    ns_dir = tmp_path / 'svc'
    ns_dir.mkdir()
    (ns_dir / 'schema.json').write_text(
        json.dumps({
            'version': '1.0',
            'type': 'object',
            'properties': {
                'nested': {
                    'type': 'object',
                    'properties': {
                        'host-name': {'type': 'string'},
                        'port-number': {'type': 'integer'},
                    },
                },
            },
        }),
    )
    output = generate(tmp_path)
    assert "    'host-name': str," in output
    assert "    'port-number': int," in output


def test_generate_empty_namespace_no_overload(tmp_path: Path) -> None:
    """Empty namespace emits a plain get() without @overload (single overload is invalid)."""
    ns_dir = tmp_path / 'empty-svc'
    ns_dir.mkdir()
    (ns_dir / 'schema.json').write_text(
        json.dumps({'version': '1.0', 'type': 'object', 'properties': {}}),
    )
    output = generate(tmp_path)
    assert '    def get(self, key: str) -> OptionValue: ...' in output
    assert '@overload\n    def get' not in output


def test_generate_is_deterministic(schemas_dir: Path) -> None:
    assert generate(schemas_dir) == generate(schemas_dir)
