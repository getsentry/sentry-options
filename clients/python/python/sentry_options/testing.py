"""Testing utilities for overriding option values.

Import this module to enable test overrides:

    from sentry_options.testing import override_options

    with override_options('namespace', {'key': 'test-value'}):
        # options('namespace').get('key') returns 'test-value'
        ...
"""
from __future__ import annotations

import contextlib
from collections.abc import Generator

from sentry_options import OptionValue
from sentry_options._core import _clear_override
from sentry_options._core import _set_override
from sentry_options._core import _validate_option


@contextlib.contextmanager
def override_options(
    namespace: str,
    overrides: dict[str, OptionValue],
) -> Generator[None]:
    """
    Override option values for testing.

    Validates that each key exists in the schema and the value matches
    the expected type before applying overrides.

    Usage:
        with override_options('seer', {'feature.enabled': True}):
            # options('seer').get('feature.enabled') returns True
            ...

    Raises:
        UnknownOptionError: If any key doesn't exist in the schema.
        SchemaError: If any value doesn't match the expected type.

    Note: Overrides are thread-local. They won't apply to spawned threads.
    """
    # Validate all overrides before applying any
    for key, value in overrides.items():
        _validate_option(namespace, key, value)

    previous: dict[str, OptionValue | None] = {}
    for key, value in overrides.items():
        previous[key] = _set_override(namespace, key, value)

    try:
        yield
    finally:
        for key in overrides:
            prev = previous[key]
            if prev is None:
                _clear_override(namespace, key)
            else:
                _set_override(namespace, key, prev)


__all__ = ['override_options']
