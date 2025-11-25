from __future__ import annotations

import shutil

from devenv import constants
from devenv.lib import config
from devenv.lib import proc
from devenv.lib import uv


def main(context: dict[str, str]) -> int:
    reporoot = context['reporoot']
    cfg = config.get_repo(reporoot)

    if not shutil.which('rustup'):
        raise SystemExit('rustup not on PATH. Did you run `direnv allow`?')

    proc.run(
        (
            'rustup',
            'toolchain',
            'install',
            'stable',
            '--profile',
            'minimal',
            '--component',
            'rustfmt',
            '--component',
            'clippy',
            '--no-self-update',
        ),
    )

    uv.install(
        cfg['uv']['version'],
        cfg['uv'][constants.SYSTEM_MACHINE],
        cfg['uv'][f"{constants.SYSTEM_MACHINE}_sha256"],
        reporoot,
    )

    print('syncing .venv ...')
    proc.run(
        (
            f"{reporoot}/.devenv/bin/uv",
            'sync',
            '--frozen',
            '--quiet',
            '--active',
            '--inexact',
        ),
    )

    print('installing pre-commit hooks ...')
    proc.run(
        (f"{reporoot}/.venv/bin/pre-commit", 'install', '--install-hooks'),
    )

    return 0
