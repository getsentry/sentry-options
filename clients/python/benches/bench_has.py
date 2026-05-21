"""Benchmarks for FeatureChecker.has() across various scenarios."""
from __future__ import annotations

import pyperf
from sentry_options import FeatureContext
from sentry_options import features
from sentry_options import init

NS = 'sentry-options-testing'

init()
checker = features(NS)

ctx_match = FeatureContext({'organization_id': 123})
ctx_disabled = FeatureContext({'organization_id': 123})
ctx_rollout_zero = FeatureContext({'organization_id': 123})
ctx_rollout_mid = FeatureContext({'is_trial': True})
ctx_no_match = FeatureContext({'organization_id': 999})
ctx_empty = FeatureContext({})

runner = pyperf.Runner()
runner.bench_func('has_enabled_100pct', checker.has, 'organizations:enabled-feature', ctx_match)
runner.bench_func('has_disabled', checker.has, 'organizations:disabled-feature', ctx_disabled)
runner.bench_func('has_rollout_zero', checker.has, 'organizations:rollout-zero', ctx_rollout_zero)
runner.bench_func('has_rollout_50pct', checker.has, 'organizations:rollout-mid', ctx_rollout_mid)
runner.bench_func('has_no_match', checker.has, 'organizations:enabled-feature', ctx_no_match)
runner.bench_func('has_undefined', checker.has, 'organizations:nonexistent', ctx_empty)
