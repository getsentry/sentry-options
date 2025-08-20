from __future__ import annotations

from typing import Any
from typing import NamedTuple


class _Option(NamedTuple):
    """not intended to be constructed directly"""
    tp: type[Any]
    default: object


class _PartialOption[T](NamedTuple):
    """intermediate builder for _Option"""
    tp: type[T]

    def default(self, o: T) -> _Option:
        return _Option(tp=self.tp, default=o)


def option[T](tp: type[T]) -> _PartialOption[T]:
    """usage: option(str).default('')

    it is done this way to force defaults to be typesafe
    """
    return _PartialOption(tp)
