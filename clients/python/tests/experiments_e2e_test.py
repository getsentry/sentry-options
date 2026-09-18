from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

import pytest
from conftest import run_isolated

NAMESPACE = 'sentry-options-testing'
REPO_ROOT = Path(__file__).resolve().parents[3]


def cli_binary() -> str | None:
    override = os.environ.get('SENTRY_OPTIONS_CLI')
    if override:
        return override
    default = REPO_ROOT / 'target' / 'debug' / 'sentry-options-cli'
    return str(default) if default.exists() else None


def build_options_dir(root: Path, binary: str) -> Path:
    fixture = REPO_ROOT / 'sentry-options'
    schemas = root / 'schemas' / NAMESPACE
    options = root / 'options' / NAMESPACE / 'default'
    schemas.mkdir(parents=True)
    options.mkdir(parents=True)
    shutil.copy(fixture / 'schemas' / NAMESPACE / 'schema.json', schemas / 'schema.json')
    shutil.copy(fixture / 'options' / NAMESPACE / 'default' / 'base.yaml', options / 'base.yaml')

    gen = root / 'gen'
    subprocess.run(
        [
            binary, 'write',
            '--schemas', str(root / 'schemas'),
            '--root', str(root / 'options'),
            '--output-format', 'json',
            '--out', str(gen),
        ],
        check=True,
        capture_output=True,
    )
    (generated,) = list(gen.glob('*.json'))
    values = root / 'values' / NAMESPACE
    values.mkdir(parents=True)
    shutil.copy(generated, values / 'values.json')
    return root


@pytest.fixture
def options_dir(tmp_path: Path) -> Path:
    binary = cli_binary()
    if binary is None:
        pytest.skip('sentry-options-cli binary not built')
    return build_options_dir(tmp_path, binary)


def test_pinned_assignments_from_cli_output(options_dir: Path) -> None:
    run_isolated(
        """
        from sentry_options import experiments, init

        init()
        checker = experiments("sentry-options-testing")

        control = checker.assign("checkout-color", {"organization_id": 1})
        assert (control.slot, control.status, control.arm) == (6, "assigned", "control"), control
        assert control.config is None

        treatment = checker.assign("checkout-color", {"organization_id": 5})
        assert (treatment.slot, treatment.status, treatment.arm) == (3, "assigned", "treatment"), treatment
        assert treatment.config == {"color": "green"}

        excluded = checker.assign("checkout-color", {"organization_id": 16})
        assert (excluded.slot, excluded.status, excluded.excluded_by) == (58, "excluded", "checkout-copy"), excluded
        """,
        options_dir,
    )


def test_exposure_carries_record_version(options_dir: Path) -> None:
    run_isolated(
        """
        from sentry_options import experiments, init

        init()
        record = experiments("sentry-options-testing").assign(
            "checkout-color", {"organization_id": 1}
        ).exposure("e2e")
        assert record["record_version"] == 1, record
        assert record["service"] == "e2e", record
        """,
        options_dir,
    )
