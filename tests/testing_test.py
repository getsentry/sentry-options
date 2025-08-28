from __future__ import annotations

import pathlib

from sentry_options.testing import override_options
from testing.option_group import empty_option_group


def test_override_options_simple(tmp_path: pathlib.Path) -> None:
    g = empty_option_group(tmp_path)
    assert g.get('example-option') == ''
    with override_options(testing={'example-option': '1'}):
        assert g.get('example-option') == '1'
    assert g.get('example-option') == ''


def test_override_options_nested(tmp_path: pathlib.Path) -> None:
    g = empty_option_group(tmp_path)
    assert g.get('example-option') == ''
    with override_options(testing={'example-option': '1'}):
        assert g.get('example-option') == '1'
        with override_options(testing={'example-option': '2'}):
            assert g.get('example-option') == '2'
        assert g.get('example-option') == '1'
    assert g.get('example-option') == ''


def test_override_options_decorator(tmp_path: pathlib.Path) -> None:
    g = empty_option_group(tmp_path)

    @override_options(testing={'example-option': '1'})
    def func(x: int) -> str:
        return f'{x}:{g.get("example-option")}'

    assert func(2) == '2:1'


def test_fallback_non_overridden(tmp_path: pathlib.Path) -> None:
    g = empty_option_group(tmp_path)

    with override_options(testing={'example-option': '1'}):
        assert g.get('example-option') == '1'
        assert g.get('typeddict-option') == {'x': 0, 'y': 0}
