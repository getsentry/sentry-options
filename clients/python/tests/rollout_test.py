"""Tests for NamespaceOptions rollout methods."""
from __future__ import annotations

import pytest
from sentry_options import options
from sentry_options.testing import override_options

NS = 'sentry-options-testing'


@pytest.mark.parametrize('rate,expected', [(0.0, False), (1.0, True), (1, True)])
def test_in_random_rollout_bounds(rate: float, expected: bool) -> None:
    opts = options(NS)
    with override_options(NS, {'float-option': rate}):
        assert all(opts.in_random_rollout('float-option') is expected for _ in range(100))


def test_in_random_rollout_partial() -> None:
    opts = options(NS)
    with override_options(NS, {'float-option': 0.5}):
        hits = sum(opts.in_random_rollout('float-option') for _ in range(10000))
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
        # x % 100000 = 82144
        (4512295198982144, False),
        # x % 100000 = 49999
        (4512295198949999, True),
        # bigint x % 100000 = 42976
        (2**160, True),
        # bigint x % 100000 = 77976
        ((2**160) + 35000, False),
    ],
)
def test_in_rollout_group_int_keys(key: int, expected: bool) -> None:
    with override_options(NS, {'float-option': 0.5}):
        assert options(NS).in_rollout_group('float-option', key) is expected


def test_in_rollout_group_string_keys() -> None:
    opts = options(NS)
    # md5('hello') buckets to 0.22994, matching sentry.options.rollout.
    with override_options(NS, {'float-option': 0.22995}):
        assert opts.in_rollout_group('float-option', 'hello') is True
    with override_options(NS, {'float-option': 0.22994}):
        assert opts.in_rollout_group('float-option', 'hello') is False


def test_in_rollout_group_is_deterministic() -> None:
    opts = options(NS)
    with override_options(NS, {'float-option': 0.5}):
        results = {opts.in_rollout_group('float-option', 'org:1') for _ in range(10)}
    assert len(results) == 1


@pytest.mark.parametrize('rate,expected', [(0.0, False), (1.0, True)])
def test_in_rollout_group_bounds(rate: float, expected: bool) -> None:
    opts = options(NS)
    with override_options(NS, {'float-option': rate}):
        assert all(opts.in_rollout_group('float-option', key) is expected for key in range(1000))


@pytest.mark.parametrize(
    'key',
    [
        1.5,
        None,
        b'bytes',
        ['org:1'],
        # Lone surrogates can't be encoded as UTF-8.
        '\ud800',
    ],
)
def test_in_rollout_group_invalid_key_is_false(key: object) -> None:
    with override_options(NS, {'float-option': 1.0}):
        assert options(NS).in_rollout_group('float-option', key) is False  # type: ignore[arg-type]


def test_rollout_non_numeric_option_is_false() -> None:
    opts = options(NS)
    assert opts.in_random_rollout('example-option') is False
    assert opts.in_rollout_group('example-option', 1) is False
    with override_options(NS, {'bool-option': True}):
        assert opts.in_random_rollout('bool-option') is False
        assert opts.in_rollout_group('bool-option', 1) is False


def test_rollout_unknown_option_is_false() -> None:
    opts = options(NS)
    assert opts.in_random_rollout('nonexistent-key') is False
    assert opts.in_rollout_group('nonexistent-key', 1) is False

    unknown = options('unknown-namespace')
    assert unknown.in_random_rollout('float-option') is False
    assert unknown.in_rollout_group('float-option', 1) is False
