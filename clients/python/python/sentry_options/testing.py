"""Testing utilities for overriding option values."""
from __future__ import annotations

import contextlib

from sentry_options._core import clear_override
from sentry_options._core import get_override
from sentry_options._core import set_override
from sentry_options._core import validate_override


@contextlib.contextmanager
def override_options(namespace, overrides):
    """
    Override option values for testing.

    Validates that each key exists in the schema and the value matches
    the expected type.

    Usage:
        with override_options('seer', {'feature.enabled': True}):
            # Production options().get() returns override
            ...

    Raises:
        UnknownOptionError: If any key doesn't exist in the schema.
        SchemaError: If any value doesn't match the expected type.
    """
    # Validate all overrides before applying any
    for key, value in overrides.items():
        validate_override(namespace, key, value)

    previous = {}
    for key, value in overrides.items():
        previous[key] = get_override(namespace, key)
        set_override(namespace, key, value)
    try:
        yield
    finally:
        for key in overrides:
            prev = previous[key]
            if prev is None:
                clear_override(namespace, key)
            else:
                set_override(namespace, key, prev)
