"""Percentage based rollout helpers governed by numeric options.

These are ports of ``sentry.options.rollout``. The governing option should be
a ``number`` option holding a rate between ``0.0`` (nobody) and ``1.0``
(everyone).

    from sentry_options.rollout import in_random_rollout, in_rollout_group

    if in_random_rollout('seer', 'sample-rate'):
        ...
    if in_rollout_group('seer', 'org-rollout-rate', org_id):
        ...
"""
from __future__ import annotations

import hashlib
import random

from sentry_options._core import options

_BUCKETS = 100000


def _rollout_rate(namespace: str, option_name: str) -> float:
    rate = options(namespace).get(option_name)
    if not isinstance(rate, (int, float)):
        return 0.0
    return rate


def in_random_rollout(namespace: str, option_name: str) -> bool:
    """
    Determine if the current operation is in a random % based rollout
    group governed by the option ``option_name`` in ``namespace``.
    """
    return random.random() < _rollout_rate(namespace, option_name)


def in_rollout_group(namespace: str, option_name: str, key: int | str) -> bool:
    """
    Determine if the current ``key`` expression is in a deterministic % based
    rollout group governed by the option ``option_name`` in ``namespace``.
    """
    if isinstance(key, str):
        int_key = int(hashlib.md5(key.encode('utf8')).hexdigest(), base=16)
    else:
        int_key = key
    return (int_key % _BUCKETS) / _BUCKETS < _rollout_rate(namespace, option_name)


__all__ = ['in_random_rollout', 'in_rollout_group']
