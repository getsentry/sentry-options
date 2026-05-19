"""Benchmarks for options.get() across all value types."""
from __future__ import annotations

from sentry_options import options


NS = 'sentry-options-testing'


def test_get_string(benchmark):
    opts = options(NS)
    benchmark(opts.get, 'example-option')


def test_get_integer(benchmark):
    opts = options(NS)
    benchmark(opts.get, 'int-option')


def test_get_float(benchmark):
    opts = options(NS)
    benchmark(opts.get, 'float-option')


def test_get_boolean(benchmark):
    opts = options(NS)
    benchmark(opts.get, 'bool-option')


def test_get_object(benchmark):
    opts = options(NS)
    benchmark(opts.get, 'object-option')


def test_get_array(benchmark):
    opts = options(NS)
    benchmark(opts.get, 'array-option')


def test_get_default_fallback(benchmark):
    opts = options(NS)
    benchmark(opts.get, 'string-option')
