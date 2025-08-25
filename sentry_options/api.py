from __future__ import annotations

from typing import Any
from typing import LiteralString

from sentry_options.schema.sentry import sentry
from sentry_options.schema.testing import testing


_groups = {
    'sentry': sentry,
    'testing': testing,
}


class OptionGroup[Name]:
    def get(self, key: LiteralString) -> object:
        raise NotImplementedError


def option_group(group: LiteralString) -> OptionGroup[Any]:
    raise NotImplementedError
