"""sentry-options: file-based runtime configuration for Sentry services.

A client library that reads configuration values from volume-mounted
ConfigMaps. Schemas (JSON) define available options with types and defaults;
values are managed in sentry-options-automator and deployed as JSON files.
The library watches for file changes and refreshes values automatically.
"""
from __future__ import annotations

from typing import Union

from sentry_options._core import init
from sentry_options._core import InitializationError
from sentry_options._core import NamespaceOptions
from sentry_options._core import options
from sentry_options._core import OptionsError
from sentry_options._core import SchemaError
from sentry_options._core import UnknownNamespaceError
from sentry_options._core import UnknownOptionError

_Primitive = Union[str, int, float, bool]
_Object = dict[str, _Primitive]
OptionValue = Union[_Primitive, _Object, list[Union[_Primitive, _Object]]]

__all__ = [
    'init',
    'InitializationError',
    'NamespaceOptions',
    'options',
    'OptionValue',
    'OptionsError',
    'SchemaError',
    'UnknownNamespaceError',
    'UnknownOptionError',
]
