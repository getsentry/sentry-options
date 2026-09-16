from __future__ import annotations

import json
from pathlib import Path

import pytest

from sentry_options import OptionsError, fetch_schemas


def _empty_repos_config(path: Path) -> None:
    path.write_text(json.dumps({'repos': {}}))


def test_fetch_schemas_creates_output_directory(tmp_path: Path) -> None:
    config = tmp_path / 'repos.json'
    output = tmp_path / 'schemas'
    _empty_repos_config(config)

    fetch_schemas(config, output)

    assert output.is_dir()


def test_fetch_schemas_rejects_existing_output_directory(tmp_path: Path) -> None:
    config = tmp_path / 'repos.json'
    output = tmp_path / 'schemas'
    _empty_repos_config(config)
    output.mkdir()

    with pytest.raises(OptionsError, match='Output directory already exists'):
        fetch_schemas(config, output)
