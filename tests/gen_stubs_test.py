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
            # untyped array/object fall back to broad types
            'array-key': {'type': 'array'},
            'object-key': {'type': 'object'},
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
            # object with properties
            'typed-obj-key': {
                'type': 'object',
                'properties': {
                    'host': {'type': 'string'},
                    'port': {'type': 'integer'},
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
    assert 'from sentry_options._core import *' in output
    assert (
        'from sentry_options._core import NamespaceOptions, OptionValue'
        in output
    )


def test_generate_typed_dict_import(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    assert 'TypedDict' in output


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


def test_generate_untyped_array_and_object(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    assert (
        'def get(self, key: Literal["array-key"]) -> list[object]'
        in output
    )
    assert (
        'def get(self, key: Literal["object-key"]) -> dict[str, object]'
        in output
    )


def test_generate_typed_array(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    assert (
        'def get(self, key: Literal["int-list-key"]) -> list[int]'
        in output
    )


def test_generate_array_of_objects(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    td = '_NamespaceOptions_my_service_obj_list_key_Item'
    assert f'class {td}(TypedDict):' in output
    assert '    url: str' in output
    assert '    weight: int' in output
    assert (
        f'def get(self, key: Literal["obj-list-key"]) -> list[{td}]'
        in output
    )


def test_generate_typed_object(schemas_dir: Path) -> None:
    output = generate(schemas_dir)
    td = '_NamespaceOptions_my_service_typed_obj_key_Dict'
    assert f'class {td}(TypedDict):' in output
    assert '    host: str' in output
    assert '    port: int' in output
    assert (
        f'def get(self, key: Literal["typed-obj-key"]) -> {td}' in output
    )


def test_generate_is_deterministic(schemas_dir: Path) -> None:
    assert generate(schemas_dir) == generate(schemas_dir)
