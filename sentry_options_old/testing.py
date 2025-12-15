from __future__ import annotations

import contextlib
from collections.abc import Generator
from typing import Any
from typing import LiteralString
from unittest import mock

from sentry_options_old.api import _groups
from sentry_options_old.api import _OptionGroup
from sentry_options_old.validate_type import is_type_valid


@contextlib.contextmanager
def override_options(**kwargs: dict[LiteralString, object]) -> Generator[None]:
    original = _OptionGroup.get

    for group, values in kwargs.items():
        if group not in _groups:
            raise ValueError(f'unknown option group: `{group}`')
        for k, v in values.items():
            if k not in _groups[group]:
                raise ValueError(f'unknown option: `{k}` (in group `{group}`)')

            tp = _groups[group][k].tp
            if not is_type_valid(tp, v):
                raise ValueError(
                    f'incorrect option type (`[{group}]{k}`): '
                    f'expected `{tp}` got `{v!r}`',
                )

    def new_get(self: _OptionGroup[Any], key: LiteralString) -> object:
        try:
            return kwargs[self.name][key]
        except KeyError:
            return original(self, key)

    with mock.patch.object(_OptionGroup, 'get', new_get):
        yield
