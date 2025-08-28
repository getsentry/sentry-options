from __future__ import annotations

import builtins
import pathlib
from typing import Literal
from unittest import mock

from sentry_options.api import _OptionGroup


def _option_group(pth: pathlib.Path) -> _OptionGroup[Literal['testing']]:
    return _OptionGroup('testing', _location=str(pth))


def test_reloading(tmp_path: pathlib.Path) -> None:
    tdir = tmp_path.joinpath('testing')
    tdir.mkdir()
    tdir.joinpath('values.json').write_text('{}')

    g = _option_group(tmp_path)
    # should have the default value
    assert g.get('example-option') == ''

    # replace the values file
    tdir.joinpath('values.json.tmp').write_text('{"example-option": "hi"}')
    tdir.joinpath('values.json.tmp').rename(tdir.joinpath('values.json'))

    # should get the fresh value
    assert g.get('example-option') == 'hi'
    # should not attempt to reload
    with mock.patch.object(builtins, 'open', side_effect=OSError):
        assert g.get('example-option') == 'hi'
