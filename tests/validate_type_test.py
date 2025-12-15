from __future__ import annotations

from typing import Any

import pytest

from sentry_options_old.validate_type import is_type_valid


@pytest.mark.parametrize(
    ('tp', 'o'),
    (
        (str, 'a string'),
        (int, 1),
        (int, False),
        (bool, False),
        (float, 1.5),
    ),
)
def test_is_type_valid_primitives_passing(tp: type[Any], o: object) -> None:
    assert is_type_valid(tp, o)


def test_is_type_valid_not_matching() -> None:
    assert is_type_valid(str, 1) is False
