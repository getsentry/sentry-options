from __future__ import annotations

import json
import os.path
from typing import Any
from typing import LiteralString

from sentry_options.schema.sentry import sentry
from sentry_options.schema.testing import testing


_groups = {
    'sentry': sentry,
    'testing': testing,
}


class _OptionGroup[Name]:
    def __init__(
            self,
            name: str,
            *,
            _location: str = '/etc/sentry-options',
    ) -> None:
        self.name = name
        self._definitions = _groups[self.name]
        self._location = _location
        self._inode = -1
        self._data: Any = {}

    def __repr__(self) -> str:
        return f'{__name__}.{option_group.__name__}({self.name!r})'

    def _ensure_data(self) -> None:
        loc = os.path.join(self._location, self.name, 'values.json')
        res = os.stat(loc)
        if self._inode == res.st_ino:
            return
        else:
            with open(loc, encoding='UTF-8') as f:
                self._data = json.load(f)
                self._inode = os.fstat(f.fileno()).st_ino

    def get(self, key: LiteralString) -> object:
        self._ensure_data()
        option = self._definitions[key]
        return self._data.get(key, option.default)


def option_group(group: LiteralString) -> _OptionGroup[Any]:
    raise NotImplementedError
