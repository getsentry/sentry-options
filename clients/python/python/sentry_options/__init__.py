"""sentry-options: a configuration library, not a service.

Provides file-based runtime configuration via volume-mounted ConfigMaps.
It is a client library consumed by services — it has no API, no endpoints,
no running processes of its own.
"""
from __future__ import annotations

from sentry_options._core import init
from sentry_options._core import InitializationError
from sentry_options._core import NamespaceOptions
from sentry_options._core import options
from sentry_options._core import OptionsError
from sentry_options._core import SchemaError
from sentry_options._core import UnknownNamespaceError
from sentry_options._core import UnknownOptionError

__all__ = [
    'init',
    'InitializationError',
    'NamespaceOptions',
    'options',
    'OptionsError',
    'SchemaError',
    'UnknownNamespaceError',
    'UnknownOptionError',
]
