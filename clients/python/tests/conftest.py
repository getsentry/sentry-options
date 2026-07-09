from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from textwrap import dedent

import pytest
from sentry_options import init


@pytest.fixture(scope='session', autouse=True)
def init_options(tmp_path_factory: pytest.TempPathFactory):
    """Initialize options once for the test session."""
    init()


def make_options_dir(root: Path, *, generated_at: str | None = None) -> Path:
    """Create an options directory with a ``test-ns`` schema and values file."""
    options_dir = root / 'opts'
    schemas_dir = options_dir / 'schemas' / 'test-ns'
    values_dir = options_dir / 'values' / 'test-ns'
    schemas_dir.mkdir(parents=True)
    values_dir.mkdir(parents=True)

    schemas_dir.joinpath('schema.json').write_text(
        json.dumps(
            {
                'version': '1.0',
                'type': 'object',
                'properties': {
                    'enabled': {
                        'type': 'boolean',
                        'default': False,
                        'description': 'Enabled',
                    },
                },
            },
        ),
    )

    values: dict = {'options': {'enabled': True}}
    if generated_at is not None:
        values['generated_at'] = generated_at
    values_dir.joinpath('values.json').write_text(json.dumps(values))

    return options_dir


def run_isolated(script: str, options_dir: Path, timeout: int = 30, **env_extra: str) -> None:
    """Run a Python script in a subprocess with its own ``SENTRY_OPTIONS_DIR``.

    Each subprocess gets a fresh Rust ``OnceLock``, so scripts can call
    ``init(...)`` with their own settings.
    """
    env = os.environ.copy()
    env['SENTRY_OPTIONS_DIR'] = str(options_dir)
    env.update(env_extra)
    result = subprocess.run(
        [sys.executable, '-c', dedent(script)],
        env=env,
        capture_output=True,
        text=True,
        timeout=timeout,
    )
    assert result.returncode == 0, (
        f"Script failed.\nstdout: {result.stdout}\nstderr: {result.stderr}"
    )
