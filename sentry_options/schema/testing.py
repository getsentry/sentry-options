from __future__ import annotations

from typing import LiteralString
from typing import TypedDict

from sentry_options.schema._option import _Option
from sentry_options.schema._option import option


class _TD(TypedDict):
    x: int
    y: int


testing: dict[LiteralString, _Option] = {
    'example-option': option(str).default(''),
    'list-option': option(list[str]).default([]),
    'dict-option': option(dict[str, int]).default({}),
    'typeddict-option': option(_TD).default({'x': 0, 'y': 0}),
}
