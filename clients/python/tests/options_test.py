"""Tests for sentry-options Python client."""
from __future__ import annotations

import pytest
from sentry_options import init
from sentry_options import InitializationError
from sentry_options import options
from sentry_options import OptionsError
from sentry_options import SchemaError
from sentry_options import UnknownNamespaceError
from sentry_options import UnknownOptionError


def test_get_string_from_values() -> None:
    value = options('sentry-options-testing').get('example-option')
    assert value == 'wow'
    assert isinstance(value, str)


def test_get_int_from_values() -> None:
    value = options('sentry-options-testing').get('int-option')
    assert value == 123
    assert isinstance(value, int)


def test_get_float_from_values() -> None:
    value = options('sentry-options-testing').get('float-option')
    assert value == 1.2
    assert isinstance(value, float)


def test_get_bool_from_values() -> None:
    value = options('sentry-options-testing').get('bool-option')
    assert value is False
    assert isinstance(value, bool)


def test_unknown_namespace() -> None:
    with pytest.raises(UnknownNamespaceError, match='nonexistent'):
        options('nonexistent').get('any-key')


def test_unknown_option() -> None:
    with pytest.raises(UnknownOptionError, match='bad-key'):
        options('sentry-options-testing').get('bad-key')


def test_double_init() -> None:
    with pytest.raises(InitializationError, match='already initialized'):
        init()


def test_exceptions_inherit_from_options_error() -> None:
    assert issubclass(SchemaError, OptionsError)
    assert issubclass(UnknownNamespaceError, OptionsError)
    assert issubclass(UnknownOptionError, OptionsError)
    assert issubclass(InitializationError, OptionsError)
