#!/usr/bin/env python3
"""
Remove register() blocks for a list of option keys from sentry/getsentry source files.

Usage:
    uv run delete-options.py --sentry ~/dev/sentry --getsentry ~/dev/getsentry [--dry-run]

Reads the list of keys to delete from stdin (one key per line, tabs ignored).
Falls back to safe-to-delete-options.py output if stdin is a tty.
"""
from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path


def find_block(content: str, key: str) -> tuple[int, int] | None:
    """
    Return (start, end) byte offsets for the full register block for `key`,
    including any directly-preceding comment lines and one trailing newline.
    Returns None if not found.
    """
    # Match both `register(` and `options.register(` styles
    pattern = re.compile(
        r'(?:options\.)?register\(\s*\n?\s*"' + re.escape(key) + r'"',
        re.MULTILINE,
    )
    m = pattern.search(content)
    if not m:
        return None

    # Find the start of the line containing the match
    line_start = content.rfind('\n', 0, m.start()) + 1

    # Walk backwards eating comment lines (lines starting with #).
    # Stop at a blank line or non-comment line — that's the boundary.
    block_start = line_start
    pos = line_start - 1  # points to the \n before this line
    while pos > 0:
        prev_nl = content.rfind('\n', 0, pos)
        prev_line = content[prev_nl + 1:pos]
        stripped = prev_line.strip()
        if stripped.startswith('#'):
            block_start = prev_nl + 1
            pos = prev_nl
        else:
            break

    # Walk forward matching parens to find the end of the register() call.
    depth = 0
    end = m.start()
    for i in range(m.start(), len(content)):
        if content[i] == '(':
            depth += 1
        elif content[i] == ')':
            depth -= 1
            if depth == 0:
                end = i + 1
                break

    # Consume one trailing newline so we don't leave a stranded blank line.
    if end < len(content) and content[end] == '\n':
        end += 1

    return block_start, end


def delete_keys_from_file(path: Path, keys: list[str], dry_run: bool) -> list[str]:
    """Remove all matching register blocks from `path`. Returns list of deleted keys."""
    content = path.read_text()
    deleted: list[str] = []

    # Collect all (start, end, key) ranges, then apply in reverse to preserve offsets.
    ranges: list[tuple[int, int, str]] = []
    for key in keys:
        result = find_block(content, key)
        if result:
            ranges.append((result[0], result[1], key))

    if not ranges:
        return []

    # Sort by start position descending so we can splice without shifting offsets.
    ranges.sort(key=lambda t: t[0], reverse=True)

    for start, end, key in ranges:
        snippet = content[start:end].strip()
        print(f"  [{path}] removing {key!r}")
        if dry_run:
            print(f"    --- preview ---")
            for line in snippet.splitlines():
                print(f"    {line}")
        else:
            content = content[:start] + content[end:]
        deleted.append(key)

    if not dry_run:
        path.write_text(content)

    return deleted


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--sentry', required=True)
    parser.add_argument('--getsentry', required=True)
    parser.add_argument('--dry-run', action='store_true',
                        help='Show what would be removed without writing files')
    args = parser.parse_args()

    sentry = Path(args.sentry).expanduser()
    getsentry = Path(args.getsentry).expanduser()

    # Read keys from stdin or fall back to safe-to-delete-options.py
    if sys.stdin.isatty():
        here = Path(__file__).parent
        result = subprocess.run(
            ['uv', 'run', str(here / 'safe-to-delete-options.py')],
            capture_output=True, text=True, cwd=here,
        )
        keys = [line.split('\t')[0] for line in result.stdout.splitlines() if line]
    else:
        keys = [line.split('\t')[0] for line in sys.stdin.read().splitlines() if line]

    if not keys:
        print("No keys to delete.", file=sys.stderr)
        sys.exit(0)

    print(f"{'DRY RUN — ' if args.dry_run else ''}Deleting {len(keys)} options from source files\n")

    # Search both repos for files containing each key's register() block.
    # We only search known registration directories to avoid hitting tests/migrations.
    search_roots = [
        sentry / 'src/sentry',
        getsentry / 'getsentry',
    ]

    # Build a map from file → keys that appear in it.
    file_keys: dict[Path, list[str]] = {}
    for key in keys:
        for root in search_roots:
            try:
                rg = subprocess.run(
                    ['rg', '-l', '--type', 'py', f'"{re.escape(key)}"', str(root)],
                    capture_output=True, text=True,
                )
                for filepath in rg.stdout.strip().splitlines():
                    if not filepath:
                        continue
                    p = Path(filepath)
                    # Skip tests and migrations — the inventory scan already excludes these,
                    # but rg can still find test fixtures that call register() for setup.
                    if '/tests/' in filepath or '/test_' in filepath or '/migrations/' in filepath:
                        continue
                    file_keys.setdefault(p, []).append(key)
            except FileNotFoundError:
                print("ERROR: rg (ripgrep) not found", file=sys.stderr)
                sys.exit(1)

    if not file_keys:
        print("No matching register() blocks found.", file=sys.stderr)
        sys.exit(0)

    total_deleted = 0
    not_found = set(keys)
    for path, file_key_list in sorted(file_keys.items()):
        deleted = delete_keys_from_file(path, file_key_list, args.dry_run)
        total_deleted += len(deleted)
        not_found -= set(deleted)

    print(f"\n{'Would delete' if args.dry_run else 'Deleted'} {total_deleted}/{len(keys)} options.")
    if not_found:
        print(f"\nNot found in any source file ({len(not_found)}):")
        for k in sorted(not_found):
            print(f"  {k}")


if __name__ == '__main__':
    main()
