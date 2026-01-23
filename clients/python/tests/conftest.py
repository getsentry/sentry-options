"""Shared pytest fixtures for sentry-options tests."""
from __future__ import annotations

import os
from pathlib import Path

import pytest
from sentry_options import init


@pytest.fixture(scope='session', autouse=True)
def init_options() -> None:
    """Initialize options using the sentry-options-testing schema/values."""
    options_dir = Path(__file__).parent.parent.parent.parent / 'sentry-options'

    orig_env = os.environ.get('SENTRY_OPTIONS_DIR')
    os.environ['SENTRY_OPTIONS_DIR'] = str(options_dir)

    init()

    yield

    if orig_env is None:
        os.environ.pop('SENTRY_OPTIONS_DIR', None)
    else:
        os.environ['SENTRY_OPTIONS_DIR'] = orig_env
