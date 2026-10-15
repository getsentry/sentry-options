"""Tests for sentry_options.rollout."""
from __future__ import annotations

import pytest
from sentry_options import UnknownNamespaceError
from sentry_options import UnknownOptionError
from sentry_options.rollout import in_random_rollout
from sentry_options.rollout import in_rollout_group
from sentry_options.testing import override_options

NS = 'sentry-options-testing'


@pytest.mark.parametrize('rate,expected', [(0.0, False), (1.0, True), (1, True)])
def test_in_random_rollout_bounds(rate: float, expected: bool) -> None:
    with override_options(NS, {'float-option': rate}):
        assert all(in_random_rollout(NS, 'float-option') is expected for _ in range(100))


def test_in_random_rollout_partial() -> None:
    with override_options(NS, {'float-option': 0.5}):
        hits = sum(in_random_rollout(NS, 'float-option') for _ in range(10000))
    assert 4000 < hits < 6000


@pytest.mark.parametrize(
    'key,expected',
    [
        (0, True),
        (49999, True),
        (50000, False),
        (99999, False),
        (100000, True),
        # -5 % 100000 == 99995
        (-5, False),
    ],
)
def test_in_rollout_group_int_keys(key: int, expected: bool) -> None:
    with override_options(NS, {'float-option': 0.5}):
        assert in_rollout_group(NS, 'float-option', key) is expected


def test_in_rollout_group_string_keys() -> None:
    # md5('hello') buckets to 0.22994, matching the Rust implementation.
    with override_options(NS, {'float-option': 0.22995}):
        assert in_rollout_group(NS, 'float-option', 'hello') is True
    with override_options(NS, {'float-option': 0.22994}):
        assert in_rollout_group(NS, 'float-option', 'hello') is False


def test_in_rollout_group_is_deterministic() -> None:
    with override_options(NS, {'float-option': 0.5}):
        results = {in_rollout_group(NS, 'float-option', 'org:1') for _ in range(10)}
    assert len(results) == 1


@pytest.mark.parametrize('rate,expected', [(0.0, False), (1.0, True)])
def test_in_rollout_group_bounds(rate: float, expected: bool) -> None:
    with override_options(NS, {'float-option': rate}):
        assert all(in_rollout_group(NS, 'float-option', key) is expected for key in range(1000))


def test_rollout_rejects_non_numeric_option() -> None:
    with pytest.raises(TypeError):
        in_random_rollout(NS, 'example-option')
    with pytest.raises(TypeError):
        in_rollout_group(NS, 'example-option', 1)
    with pytest.raises(TypeError):
        in_rollout_group(NS, 'bool-option', 1)


def test_rollout_unknown_option() -> None:
    with pytest.raises(UnknownOptionError):
        in_rollout_group(NS, 'nonexistent-key', 1)
    with pytest.raises(UnknownNamespaceError):
        in_random_rollout('unknown-namespace', 'float-option')
