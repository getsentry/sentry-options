"""Testing utilities for overriding option values.

Import this module to enable test overrides:

    from sentry_options.testing import override_options

    with override_options('namespace', {'key': 'test-value'}):
        # options('namespace').get('key') returns 'test-value'
        ...
"""
from __future__ import annotations

import contextlib
import threading
from collections.abc import Generator
from typing import Any

from sentry_options._core import _register_override_hook
from sentry_options._core import _validate_option

# Thread-local override storage
_local = threading.local()


def _get_overrides() -> dict[tuple[str, str], Any]:
    if not hasattr(_local, 'overrides'):
        _local.overrides = {}
    return _local.overrides


def _override_hook(namespace: str, key: str) -> Any:
    """Called by Rust extension on every get() when hook is registered."""
    return _get_overrides().get((namespace, key))


# Register hook when this module is imported
# Silently ignore if already registered (e.g., multiple imports)
try:
    _register_override_hook(_override_hook)
except RuntimeError:
    pass


@contextlib.contextmanager
def override_options(
    namespace: str,
    overrides: dict[str, Any],
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
    # Validate all overrides before applying any (auto-inits if needed)
    for key, value in overrides.items():
        _validate_option(namespace, key, value)

    store = _get_overrides()
    previous: dict[str, Any] = {}

    for key, value in overrides.items():
        previous[key] = store.get((namespace, key))
        store[(namespace, key)] = value

    try:
        yield
    finally:
        for key in overrides:
            prev = previous[key]
            if prev is None:
                store.pop((namespace, key), None)
            else:
                store[(namespace, key)] = prev


__all__ = ['override_options']
