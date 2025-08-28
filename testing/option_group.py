from __future__ import annotations

import pathlib
from typing import Literal

from sentry_options.api import _OptionGroup


def empty_option_group(
        tmp_path: pathlib.Path,
) -> _OptionGroup[Literal['testing']]:
    tdir = tmp_path.joinpath('testing')
    tdir.mkdir()
    tdir.joinpath('values.json').write_text('{}')
    return _OptionGroup('testing', _location=str(tmp_path))
