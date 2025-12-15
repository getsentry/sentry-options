from __future__ import annotations

import json
import os

import pytest
from sentry_options import init
from sentry_options import InitializationError
from sentry_options import options
from sentry_options import OptionsError
from sentry_options import SchemaError
from sentry_options import UnknownNamespaceError
from sentry_options import UnknownOptionError


@pytest.fixture(scope='module', autouse=True)
def init_options(tmp_path_factory: pytest.TempPathFactory) -> None:
    """Set up test data and initialize options."""
    tmpdir = tmp_path_factory.mktemp('sentry_options')

    # Create schema
    schema_dir = tmpdir / 'schemas' / 'testing'
    schema_dir.mkdir(parents=True)
    (schema_dir / 'schema.json').write_text(
        json.dumps(
            {
                'version': '1.0',
                'type': 'object',
                'properties': {
                    'str-opt': {
                        'type': 'string',
                        'default': 'default-value',
                        'description': 'A string option',
                    },
                    'int-opt': {
                        'type': 'integer',
                        'default': 42,
                        'description': 'An integer option',
                    },
                    'float-opt': {
                        'type': 'number',
                        'default': 3.14,
                        'description': 'A float option',
                    },
                    'bool-opt': {
                        'type': 'boolean',
                        'default': True,
                        'description': 'A boolean option',
                    },
                },
            },
        ),
    )

    # Create values (override str-opt)
    values_dir = tmpdir / 'values' / 'testing'
    values_dir.mkdir(parents=True)
    values = {'str-opt': 'custom-value'}
    (values_dir / 'values.json').write_text(json.dumps(values))

    # Set env var and initialize
    orig_env = os.environ.get('SENTRY_OPTIONS_DIR')
    os.environ['SENTRY_OPTIONS_DIR'] = str(tmpdir)

    init()

    yield

    # Restore env var
    if orig_env is None:
        os.environ.pop('SENTRY_OPTIONS_DIR', None)
    else:
        os.environ['SENTRY_OPTIONS_DIR'] = orig_env


def test_get_string_from_values() -> None:
    value = options('testing').get('str-opt')
    assert value == 'custom-value'
    assert isinstance(value, str)


def test_get_int_default() -> None:
    value = options('testing').get('int-opt')
    assert value == 42
    assert isinstance(value, int)


def test_get_float_default() -> None:
    value = options('testing').get('float-opt')
    assert value == 3.14
    assert isinstance(value, float)


def test_get_bool_default() -> None:
    value = options('testing').get('bool-opt')
    assert value is True
    assert isinstance(value, bool)


def test_unknown_namespace() -> None:
    with pytest.raises(UnknownNamespaceError, match='nonexistent'):
        options('nonexistent').get('any-key')


def test_unknown_option() -> None:
    with pytest.raises(UnknownOptionError, match='bad-key'):
        options('testing').get('bad-key')


def test_double_init() -> None:
    with pytest.raises(InitializationError, match='already initialized'):
        init()


def test_exceptions_inherit_from_options_error() -> None:
    assert issubclass(SchemaError, OptionsError)
    assert issubclass(UnknownNamespaceError, OptionsError)
    assert issubclass(UnknownOptionError, OptionsError)
    assert issubclass(InitializationError, OptionsError)
