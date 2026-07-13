"""E2E tests for manual refresh and the configurable staleness threshold.

Each test spins up a subprocess with its own ``SENTRY_OPTIONS_DIR`` so the
Rust ``OnceLock`` starts fresh and ``init(refresh_threshold=...)`` takes
effect.
"""
from __future__ import annotations

from pathlib import Path

from conftest import make_options_dir
from conftest import run_isolated


def test_manual_refresh_with_disabled_threshold(tmp_path: Path) -> None:
    options_dir = make_options_dir(tmp_path)
    values_file = options_dir / 'values' / 'test-ns' / 'values.json'
    run_isolated(
        f"""\
        import json
        from sentry_options import init, options, refresh

        init(refresh_threshold=None)
        opts = options("test-ns")
        assert opts.get("enabled") is True

        with open("{values_file}", "w") as f:
            json.dump({{"options": {{"enabled": False}}}}, f)

        # With refresh-on-read disabled, reads never pick up the change.
        assert opts.get("enabled") is True
        # A manual refresh does.
        assert refresh() is True
        assert opts.get("enabled") is False
        # Nothing changed since the last refresh.
        assert refresh() is False
        """,
        options_dir,
    )


def test_refresh_before_init_raises(tmp_path: Path) -> None:
    options_dir = make_options_dir(tmp_path)
    run_isolated(
        """\
        from sentry_options import NotInitializedError, refresh

        try:
            refresh()
        except NotInitializedError:
            pass
        else:
            raise AssertionError("Expected NotInitializedError")
        """,
        options_dir,
    )


def test_init_rejects_negative_refresh_threshold(tmp_path: Path) -> None:
    options_dir = make_options_dir(tmp_path)
    run_isolated(
        """\
        from sentry_options import init

        try:
            init(refresh_threshold=-1.0)
        except ValueError:
            pass
        else:
            raise AssertionError("Expected ValueError")
        """,
        options_dir,
    )
