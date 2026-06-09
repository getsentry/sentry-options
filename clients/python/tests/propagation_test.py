"""E2E tests for the propagation time callback through the Python client.

Each test spins up a subprocess with its own ``SENTRY_OPTIONS_DIR`` so the
Rust ``OnceLock`` starts fresh and ``init(on_propagation=...)`` can register
a real Python callback.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
from pathlib import Path
from textwrap import dedent

# 5 s refresh threshold + up to 1 s jitter + margin
REFRESH_WAIT = 7


def _make_options_dir(root: Path, *, generated_at: str | None = None) -> Path:
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


def _run(script: str, options_dir: Path, timeout: int = 30, **env_extra: str) -> None:
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


def test_propagation_callback_fires_on_generated_at_change(tmp_path: Path) -> None:
    options_dir = _make_options_dir(tmp_path, generated_at='2024-01-21T18:30:00+00:00')
    values_file = options_dir / 'values' / 'test-ns' / 'values.json'
    _run(
        f"""\
        import json, time
        from sentry_options import init, options

        results = []
        init(on_propagation=lambda ns, delay: results.append((ns, delay)))
        assert options("test-ns").get("enabled") is True

        time.sleep({REFRESH_WAIT})
        with open("{values_file}", "w") as f:
            json.dump({{"options": {{"enabled": True}}, "generated_at": "2024-01-21T19:00:00+00:00"}}, f)
        options("test-ns").get("enabled")

        assert len(results) == 1, f"Expected 1 callback, got {{len(results)}}: {{results}}"
        ns, delay = results[0]
        assert ns == "test-ns"
        assert isinstance(delay, float)
        assert delay > 0.0
        """,
        options_dir,
    )


def test_propagation_callback_not_called_without_generated_at(tmp_path: Path) -> None:
    _run(
        f"""\
        import time
        from sentry_options import init, options

        results = []
        init(on_propagation=lambda ns, delay: results.append((ns, delay)))
        options("test-ns").get("enabled")

        time.sleep({REFRESH_WAIT})
        options("test-ns").get("enabled")

        assert len(results) == 0, f"Expected no callbacks, got {{len(results)}}"
        """,
        _make_options_dir(tmp_path),
    )


def test_propagation_callback_exception_does_not_crash(tmp_path: Path) -> None:
    options_dir = _make_options_dir(tmp_path, generated_at='2024-01-21T18:30:00+00:00')
    values_file = options_dir / 'values' / 'test-ns' / 'values.json'
    _run(
        f"""\
        import json, time
        from sentry_options import init, options

        def boom(ns, delay):
            raise RuntimeError("callback boom")

        init(on_propagation=boom)
        assert options("test-ns").get("enabled") is True

        time.sleep({REFRESH_WAIT})
        with open("{values_file}", "w") as f:
            json.dump({{"options": {{"enabled": True}}, "generated_at": "2024-01-21T19:00:00+00:00"}}, f)

        val = options("test-ns").get("enabled")
        assert val is True
        """,
        options_dir,
    )


def test_propagation_callback_receives_multiple_updates(tmp_path: Path) -> None:
    options_dir = _make_options_dir(tmp_path, generated_at='2024-01-21T18:00:00+00:00')
    values_file = options_dir / 'values' / 'test-ns' / 'values.json'
    _run(
        f"""\
        import json, time
        from sentry_options import init, options

        results = []
        init(on_propagation=lambda ns, delay: results.append((ns, delay)))
        options("test-ns").get("enabled")

        time.sleep({REFRESH_WAIT})
        with open("{values_file}", "w") as f:
            json.dump({{"options": {{"enabled": True}}, "generated_at": "2024-01-21T19:00:00+00:00"}}, f)
        options("test-ns").get("enabled")
        assert len(results) == 1, f"After 1st update: expected 1, got {{len(results)}}"

        time.sleep({REFRESH_WAIT})
        with open("{values_file}", "w") as f:
            json.dump({{"options": {{"enabled": True}}, "generated_at": "2024-01-21T20:00:00+00:00"}}, f)
        options("test-ns").get("enabled")
        assert len(results) == 2, f"After 2nd update: expected 2, got {{len(results)}}"

        assert all(ns == "test-ns" for ns, _ in results)
        assert all(d > 0.0 for _, d in results)
        """,
        options_dir,
        timeout=45,
    )
