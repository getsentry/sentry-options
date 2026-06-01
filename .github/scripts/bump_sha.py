#!/usr/bin/env python3
"""Bump a repo's SHA in sentry-options-automator's repos.json and open/update a PR."""
from __future__ import annotations

import json
import os
import subprocess
import sys


def run(cmd: list[str], *, cwd: str | None = None, capture: bool = False) -> str:
    result = subprocess.run(
        cmd,
        cwd=cwd,
        stdout=subprocess.PIPE if capture else None,
        text=True,
        check=True,
    )
    return result.stdout.strip() if capture else ''


def main() -> None:
    schemas_path = os.environ['SCHEMAS_PATH']
    repo_name = os.environ['REPO_NAME']
    new_sha = os.environ['NEW_SHA']
    # -- Verify schemas directory exists in calling repo --
    if not os.path.isdir(schemas_path):
        print(f"::error::Schemas directory '{schemas_path}' not found")
        sys.exit(1)

    # -- Read repos.json --
    repos_path = os.path.join('automator', 'repos.json')
    with open(repos_path) as f:
        data = json.load(f)

    repos = data['repos']
    if repo_name not in repos:
        print(f"::error::Repository '{repo_name}' not found in repos.json")
        sys.exit(1)

    current_sha = repos[repo_name].get('sha', '')
    if current_sha == new_sha:
        print(f"SHA already up to date ({new_sha}), nothing to do.")
        return

    # -- Update SHA --
    repos[repo_name]['sha'] = new_sha
    with open(repos_path, 'w') as f:
        json.dump(data, f, indent=2)
        f.write('\n')
    print(f"Updated {repo_name} SHA: {current_sha} -> {new_sha}")

    # -- Commit and push --
    short_sha = new_sha[:12]
    branch = f"auto/bump-{repo_name}"
    title = f"chore: bump {repo_name} schema SHA to {short_sha}"
    cwd = 'automator'

    run(['git', 'config', 'user.name', 'sentry-internal[bot]'], cwd=cwd)
    run(['git', 'config', 'user.email', '38422159+sentry-internal[bot]@users.noreply.github.com'], cwd=cwd)
    run(['git', 'checkout', '-B', branch], cwd=cwd)
    run(['git', 'add', 'repos.json'], cwd=cwd)
    run(['git', 'commit', '-m', title], cwd=cwd)
    run(['git', 'push', '--force', 'origin', branch], cwd=cwd)

    # -- Create or update PR --
    existing_pr = run(
        [
            'gh', 'pr', 'list', '--repo', 'getsentry/sentry-options-automator',
            '--head', branch, '--state', 'open',
            '--json', 'number', '--jq', '.[0].number // empty',
        ],
        capture=True,
    )

    if existing_pr:
        run([
            'gh', 'pr', 'edit', existing_pr,
            '--repo', 'getsentry/sentry-options-automator',
            '--title', title, '--add-label', 'auto-merge',
        ])
        print(f"Updated existing PR #{existing_pr}")
    else:
        body = (
            f"Automated SHA bump for **{repo_name}** to "
            f"[`{short_sha}`](https://github.com/getsentry/{repo_name}/commit/{new_sha}).\n\n"
            f"Triggered by merge to `main` in "
            f"[getsentry/{repo_name}](https://github.com/getsentry/{repo_name})."
        )
        pr_url = run(
            [
                'gh', 'pr', 'create', '--repo', 'getsentry/sentry-options-automator',
                '--head', branch, '--base', 'main',
                '--title', title, '--body', body, '--label', 'auto-merge',
            ],
            capture=True,
        )
        print(f"Created PR: {pr_url}")


if __name__ == '__main__':
    main()
