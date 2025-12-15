from __future__ import annotations

from typing import Any


def is_type_valid(tp: type[Any], value: object) -> bool:
    if tp in (int, float, str, bool):  # simple validation for primitives
        return isinstance(value, tp)
    else:
        raise NotImplementedError(tp)  # TODO: handle validating other types
