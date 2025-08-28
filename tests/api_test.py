from __future__ import annotations

import builtins
import pathlib
from unittest import mock

from testing.option_group import empty_option_group


def test_reloading(tmp_path: pathlib.Path) -> None:
    g = empty_option_group(tmp_path)
    # should have the default value
    assert g.get('example-option') == ''

    # replace the values file
    tdir = tmp_path.joinpath('testing')
    tdir.joinpath('values.json.tmp').write_text('{"example-option": "hi"}')
    tdir.joinpath('values.json.tmp').rename(tdir.joinpath('values.json'))

    # should get the fresh value
    assert g.get('example-option') == 'hi'
    # should not attempt to reload
    with mock.patch.object(builtins, 'open', side_effect=OSError):
        assert g.get('example-option') == 'hi'
