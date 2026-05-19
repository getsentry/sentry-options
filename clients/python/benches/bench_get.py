"""Benchmarks for options.get() across all value types."""
from __future__ import annotations

import pyperf
from sentry_options import init
from sentry_options import options

NS = 'sentry-options-testing'

init()
opts = options(NS)

runner = pyperf.Runner()
runner.bench_func('get_string', opts.get, 'example-option')
runner.bench_func('get_integer', opts.get, 'int-option')
runner.bench_func('get_float', opts.get, 'float-option')
runner.bench_func('get_boolean', opts.get, 'bool-option')
runner.bench_func('get_object', opts.get, 'object-option')
runner.bench_func('get_array', opts.get, 'array-option')
runner.bench_func('get_default_fallback', opts.get, 'string-option')
