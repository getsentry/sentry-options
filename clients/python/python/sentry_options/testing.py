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
from typing import Any

from sentry_options import OptionValue
from sentry_options._core import _clear_override
from sentry_options._core import _set_override
from sentry_options._core import _validate_experiments
from sentry_options._core import _validate_option

# A feature flag value (the `Feature` object: owner/created_at/enabled/segments).
# Deeply nested arbitrary JSON, so it's typed loosely rather than as OptionValue.
Feature = dict[str, Any]


def always_on() -> Feature:
    """A feature value that is enabled for every context.

    Use with :func:`override_options` to force a flag on in tests::

        with override_options('seer', {'feature.my-flag': always_on()}):
            assert features('seer').has('my-flag', ctx)
    """
    return {
        'owner': {'team': 'testing'},
        'created_at': '1970-01-01',
        'segments': [{'name': 'always-on', 'rollout': 100, 'conditions': []}],
    }


def always_off() -> Feature:
    """A feature value that is disabled for every context.

    Use with :func:`override_options` to force a flag off in tests.
    """
    return {
        'owner': {'team': 'testing'},
        'created_at': '1970-01-01',
        'enabled': False,
        'segments': [],
    }


# An experiment value (owner/layer/unit/allocation/enabled/arms).
Experiment = dict[str, Any]


def experiment(
    *,
    layer: str,
    unit: list[str],
    arms: dict[str, int],
    start: int = 0,
    size: int = 100,
    enabled: bool = True,
    team: str = 'testing',
) -> Experiment:
    return {
        'owner': {'team': team},
        'layer': layer,
        'unit': unit,
        'allocation': {'start': start, 'size': size},
        'enabled': enabled,
        'arms': [{'name': name, 'weight': weight} for name, weight in arms.items()],
    }


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
    # Shape-check every key before touching thread-local state.
    for key, value in overrides.items():
        _validate_option(namespace, key, value)

    previous: dict[str, OptionValue | None] = {}
    for key, value in overrides.items():
        previous[key] = _set_override(namespace, key, value)

    def restore() -> None:
        for key in overrides:
            prev = previous[key]
            if prev is None:
                _clear_override(namespace, key)
            else:
                _set_override(namespace, key, prev)

    # With the whole batch applied, check the layer; an overlapping batch fails
    # here and nothing is left applied.
    try:
        _validate_experiments(namespace)
    except Exception:
        restore()
        raise

    try:
        yield
    finally:
        restore()


__all__ = [
    'Experiment',
    'Feature',
    'always_off',
    'always_on',
    'experiment',
    'override_options',
]
