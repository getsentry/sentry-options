"""Tests for sentry_options.testing override functionality."""
from __future__ import annotations

import pytest
from sentry_options import options
from sentry_options import SchemaError
from sentry_options.testing import override_options


# Simulates production code that reads options
def get_int_option():
    return options('sentry-options-testing').get('int-option')


def get_bool_option():
    return options('sentry-options-testing').get('bool-option')


def test_override_production_code() -> None:
    """Verify overrides intercept production code that uses options().get()."""
    assert get_int_option() == 123
    assert get_bool_option() is False

    overrides = {'int-option': 999, 'bool-option': True}
    with override_options('sentry-options-testing', overrides):
        assert get_int_option() == 999
        assert get_bool_option() is True

    assert get_int_option() == 123
    assert get_bool_option() is False


def test_override_intercepts_get() -> None:
    opts = options('sentry-options-testing')
    assert opts.get('int-option') == 123

    with override_options('sentry-options-testing', {'int-option': 999}):
        assert opts.get('int-option') == 999

    assert opts.get('int-option') == 123


def test_override_multiple_keys() -> None:
    overrides = {'int-option': 100, 'bool-option': True}
    with override_options('sentry-options-testing', overrides):
        opts = options('sentry-options-testing')
        assert opts.get('int-option') == 100
        assert opts.get('bool-option') is True


def test_override_nesting() -> None:
    opts = options('sentry-options-testing')

    with override_options('sentry-options-testing', {'int-option': 200}):
        assert opts.get('int-option') == 200

        with override_options('sentry-options-testing', {'int-option': 300}):
            assert opts.get('int-option') == 300

        assert opts.get('int-option') == 200

    assert opts.get('int-option') == 123


def test_override_exception_cleanup() -> None:
    opts = options('sentry-options-testing')

    try:
        with override_options('sentry-options-testing', {'bool-option': True}):
            assert opts.get('bool-option') is True
            raise ValueError('intentional')
    except ValueError:
        pass

    assert opts.get('bool-option') is False


def test_override_unknown_option_raises() -> None:
    with pytest.raises(SchemaError, match='Unknown option'):
        overrides = {'nonexistent-opt': 123}
        with override_options('sentry-options-testing', overrides):
            pass


def test_override_wrong_type_raises() -> None:
    with pytest.raises(SchemaError, match='not of type'):
        overrides = {'int-option': 'not-an-int'}
        with override_options('sentry-options-testing', overrides):
            pass
