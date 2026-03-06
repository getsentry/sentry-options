"""Shared pytest fixtures for sentry-options tests."""
from __future__ import annotations

import pytest
from sentry_options import init


@pytest.fixture(scope='session', autouse=True)
def _init_options() -> None:
    """Initialize options once for the test session."""
    init()
