from __future__ import annotations

import pytest
from sentry_options import init


@pytest.fixture(scope='session', autouse=True)
def init_options(tmp_path_factory: pytest.TempPathFactory):
    """Initialize options once for the test session."""
    init()
