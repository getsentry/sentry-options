from __future__ import annotations

import contextlib
from collections.abc import Generator
from typing import Any
from typing import LiteralString
from unittest import mock

from sentry_options.api import _OptionGroup


@contextlib.contextmanager
def override_options(**kwargs: dict[LiteralString, object]) -> Generator[None]:
    original = _OptionGroup.get

    # TODO: validate kwargs against types

    def new_get(self: _OptionGroup[Any], key: LiteralString) -> object:
        try:
            return kwargs[self.name][key]
        except KeyError:
            return original(self, key)

    with mock.patch.object(_OptionGroup, 'get', new_get):
        yield
