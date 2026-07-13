"""E2E tests for the propagation time callback through the Python client.

Each test spins up a subprocess with its own ``SENTRY_OPTIONS_DIR`` so the
Rust ``OnceLock`` starts fresh and ``init(on_propagation=...)`` can register
a real Python callback. Refreshes are triggered synchronously with
``get_forced``, which bypasses the refresh threshold so the tests don't have
to wait it out.
"""
from __future__ import annotations

from pathlib import Path

from conftest import make_options_dir
from conftest import run_isolated


def test_propagation_callback_fires_on_generated_at_change(tmp_path: Path) -> None:
    options_dir = make_options_dir(tmp_path, generated_at='2024-01-21T18:30:00+00:00')
    values_file = options_dir / 'values' / 'test-ns' / 'values.json'
    run_isolated(
        f"""\
        import json
        from sentry_options import init, options

        results = []
        init(on_propagation=lambda ns, delay: results.append((ns, delay)))
        assert options("test-ns").get("enabled") is True

        with open("{values_file}", "w") as f:
            json.dump({{"options": {{"enabled": True}}, "generated_at": "2024-01-21T19:00:00+00:00"}}, f)
        options("test-ns").get_forced("enabled")

        assert len(results) == 1, f"Expected 1 callback, got {{len(results)}}: {{results}}"
        ns, delay = results[0]
        assert ns == "test-ns"
        assert isinstance(delay, float)
        assert delay > 0.0
        """,
        options_dir,
    )


def test_get_forced_sees_change_without_waiting(tmp_path: Path) -> None:
    options_dir = make_options_dir(tmp_path)
    values_file = options_dir / 'values' / 'test-ns' / 'values.json'
    run_isolated(
        f"""\
        import json
        from sentry_options import init, options

        init()
        opts = options("test-ns")
        assert opts.get("enabled") is True

        with open("{values_file}", "w") as f:
            json.dump({{"options": {{"enabled": False}}}}, f)

        # Cached get() still returns the old value within the 5s threshold.
        assert opts.get("enabled") is True
        # get_forced() bypasses the threshold and sees the change, no sleep needed.
        assert opts.get_forced("enabled") is False
        """,
        options_dir,
    )


def test_propagation_callback_not_called_without_generated_at(tmp_path: Path) -> None:
    options_dir = make_options_dir(tmp_path)
    values_file = options_dir / 'values' / 'test-ns' / 'values.json'
    run_isolated(
        f"""\
        import json
        from sentry_options import init, options

        results = []
        init(on_propagation=lambda ns, delay: results.append((ns, delay)))
        options("test-ns").get("enabled")

        # Change the values (forcing a reload) but with no generated_at.
        with open("{values_file}", "w") as f:
            json.dump({{"options": {{"enabled": False}}}}, f)
        options("test-ns").get_forced("enabled")

        assert len(results) == 0, f"Expected no callbacks, got {{len(results)}}"
        """,
        options_dir,
    )


def test_propagation_callback_exception_does_not_crash(tmp_path: Path) -> None:
    options_dir = make_options_dir(tmp_path, generated_at='2024-01-21T18:30:00+00:00')
    values_file = options_dir / 'values' / 'test-ns' / 'values.json'
    run_isolated(
        f"""\
        import json
        from sentry_options import init, options

        def boom(ns, delay):
            raise RuntimeError("callback boom")

        init(on_propagation=boom)
        assert options("test-ns").get("enabled") is True

        with open("{values_file}", "w") as f:
            json.dump({{"options": {{"enabled": True}}, "generated_at": "2024-01-21T19:00:00+00:00"}}, f)

        val = options("test-ns").get_forced("enabled")
        assert val is True
        """,
        options_dir,
    )


def test_propagation_callback_receives_multiple_updates(tmp_path: Path) -> None:
    options_dir = make_options_dir(tmp_path, generated_at='2024-01-21T18:00:00+00:00')
    values_file = options_dir / 'values' / 'test-ns' / 'values.json'
    run_isolated(
        f"""\
        import json
        from sentry_options import init, options

        results = []
        init(on_propagation=lambda ns, delay: results.append((ns, delay)))
        options("test-ns").get("enabled")

        with open("{values_file}", "w") as f:
            json.dump({{"options": {{"enabled": True}}, "generated_at": "2024-01-21T19:00:00+00:00"}}, f)
        options("test-ns").get_forced("enabled")
        assert len(results) == 1, f"After 1st update: expected 1, got {{len(results)}}"

        with open("{values_file}", "w") as f:
            json.dump({{"options": {{"enabled": True}}, "generated_at": "2024-01-21T20:00:00+00:00"}}, f)
        options("test-ns").get_forced("enabled")
        assert len(results) == 2, f"After 2nd update: expected 2, got {{len(results)}}"

        assert all(ns == "test-ns" for ns, _ in results)
        assert all(d > 0.0 for _, d in results)
        """,
        options_dir,
    )
