"""Pre-commit hook: verify _core.pyi stub matches the runtime _core module."""
from __future__ import annotations

import ast
import sys
from pathlib import Path

STUB_PATH = Path(__file__).resolve().parent.parent / 'python' / 'sentry_options' / '_core.pyi'
# Names that are expected in the runtime module but not in the stub
IGNORE_RUNTIME = {'__all__', '__doc__', '__file__', '__loader__', '__name__', '__package__', '__spec__'}


def _stub_names() -> set[str]:
    tree = ast.parse(STUB_PATH.read_text())
    names: set[str] = set()
    for node in ast.iter_child_nodes(tree):
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            names.add(node.name)
        elif isinstance(node, ast.ClassDef):
            names.add(node.name)
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name):
            names.add(node.target.id)
    return names


def _runtime_names() -> set[str]:
    import sentry_options._core as _core  # type: ignore[import-not-found]
    return {n for n in dir(_core) if not n.startswith('__')} - IGNORE_RUNTIME


def main() -> int:
    stub = _stub_names()
    try:
        runtime = _runtime_names()
    except ImportError:
        print('WARNING: sentry_options._core not importable, skipping runtime check')
        return 0

    missing_from_stub = runtime - stub
    extra_in_stub = stub - runtime

    ok = True
    if missing_from_stub:
        print(f'ERROR: runtime names missing from _core.pyi: {sorted(missing_from_stub)}')
        ok = False
    if extra_in_stub:
        print(f'ERROR: _core.pyi declares names not in runtime: {sorted(extra_in_stub)}')
        ok = False

    if ok:
        print('OK: _core.pyi stub matches runtime module')
    return 0 if ok else 1


if __name__ == '__main__':
    raise SystemExit(main())
