"""Benchmarks for options.isset() with set and unset keys."""
from __future__ import annotations

import pyperf
from sentry_options import init
from sentry_options import options

NS = 'sentry-options-testing'

init()
opts = options(NS)

runner = pyperf.Runner()
runner.bench_func('isset_set_key', opts.isset, 'example-option')
runner.bench_func('isset_unset_key', opts.isset, 'string-option')
