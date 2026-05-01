#!/usr/bin/env python3
"""
Extracts all registered options from sentry and getsentry,
finds their type and every file that calls options.get() with that key.

Usage:
    python scripts/migration/option_inventory.py \
        --sentry ~/code/sentry \
        --getsentry ~/code/getsentry \
        > inventory.csv
"""
from __future__ import annotations

import argparse
import csv
import os
import re
import subprocess
import sys
from collections import Counter


def extract_registrations(file_path: str) -> list[dict]:
    """Extract all register() calls from a file, capturing key, type, default, and flags."""
    with open(file_path) as f:
        content = f.read()

    results = []
    pattern = re.compile(r'(?:options\.)?register\(\s*\n?\s*"([^"]+)"', re.MULTILINE)

    for match in pattern.finditer(content):
        key = match.group(1)
        start = match.start()
        depth = 0
        end = start
        for i in range(start, len(content)):
            if content[i] == '(':
                depth += 1
            elif content[i] == ')':
                depth -= 1
                if depth == 0:
                    end = i + 1
                    break

        block = content[start:end]

        type_match = re.search(r'type=(\w+)', block)
        opt_type = type_match.group(1) if type_match else 'inferred'

        flags_match = re.search(r'flags=([\w\s|]+?)(?:,|\))', block)
        flags = flags_match.group(1).strip() if flags_match else 'DEFAULT_FLAGS'

        default_match = re.search(r'default=([^,\)]+)', block)
        default = default_match.group(1).strip() if default_match else ''

        results.append({
            'key': key,
            'type': opt_type,
            'flags': flags,
            'default': default,
            'source': file_path,
        })

    return results


def build_usage_index(keys: list[str], search_dirs: list[str]) -> dict[str, list[str]]:
    """Build a reverse index of key -> files by doing a single rg pass per directory."""
    escaped_keys = [re.escape(k) for k in keys]
    chunk_size = 200
    usage_map: dict[str, set[str]] = {k: set() for k in keys}

    for search_dir in search_dirs:
        for i in range(0, len(escaped_keys), chunk_size):
            chunk = escaped_keys[i: i + chunk_size]
            pattern = '|'.join(chunk)
            try:
                result = subprocess.run(
                    ['rg', '--type', 'py', '--no-heading', '-n', pattern, search_dir],
                    capture_output=True,
                    text=True,
                    timeout=120,
                )
                for line in result.stdout.split('\n'):
                    if not line:
                        continue
                    parts = line.split(':', 2)
                    if len(parts) < 3:
                        continue
                    filepath = parts[0]
                    content = parts[2]
                    for key in keys[i: i + chunk_size]:
                        if key in content:
                            usage_map[key].add(filepath)
            except (subprocess.TimeoutExpired, FileNotFoundError):
                pass

    return {k: sorted(v) for k, v in usage_map.items()}


def get_sha(repo_path: str) -> str:
    try:
        result = subprocess.run(
            ['git', '-C', repo_path, 'rev-parse', 'HEAD'],
            capture_output=True,
            text=True,
            timeout=5,
        )
        return result.stdout.strip()
    except Exception:
        return 'unknown'


def main():
    parser = argparse.ArgumentParser(description='Extract option inventory')
    parser.add_argument('--sentry', required=True, help='Path to sentry repo')
    parser.add_argument('--getsentry', required=True, help='Path to getsentry repo')
    parser.add_argument(
        '--skip-usages',
        action='store_true',
        help='Skip finding file usages (much faster)',
    )
    args = parser.parse_args()

    sentry_src = os.path.join(args.sentry, 'src/sentry')
    getsentry_src = os.path.join(args.getsentry, 'getsentry')
    search_dirs = [sentry_src, getsentry_src]

    # Find all files that register options using ripgrep
    registration_files: set[str] = set()
    for search_dir in search_dirs:
        try:
            result = subprocess.run(
                ['rg', '-l', '--type', 'py', r'options\.register\(|^register\(', search_dir],
                capture_output=True,
                text=True,
                timeout=10,
            )
            for line in result.stdout.strip().split('\n'):
                if line:
                    if '/tests/' in line or '/test_' in line or '/migrations/' in line:
                        continue
                    if line.endswith('manager.py') and 'options/manager.py' in line:
                        continue
                    registration_files.add(line)
        except (subprocess.TimeoutExpired, FileNotFoundError):
            pass

    # Note: features/manager.py auto-registers feature.* options at runtime
    # but those are covered by the feature flag inventory, not here.

    registrations: list[dict] = []
    for reg_file in sorted(registration_files):
        registrations.extend(extract_registrations(reg_file))

    # Sort by key name
    registrations.sort(key=lambda r: r['key'])

    # Shorten source paths
    for reg in registrations:
        source = reg['source']
        for prefix in [args.sentry + '/', args.getsentry + '/']:
            if source.startswith(prefix):
                source = source[len(prefix):]
                break
        reg['source'] = source

    # Find usages with batched rg (much faster than per-key)
    if not args.skip_usages:
        all_keys = [reg['key'] for reg in registrations]
        raw_usage_map = build_usage_index(all_keys, search_dirs)
        usage_map = {}
        for key, files in raw_usage_map.items():
            files = [
                f
                for f in files
                if not f.endswith('defaults.py') and not f.endswith('models/__init__.py')
            ]
            usage_map[key] = '; '.join(files)
    else:
        usage_map = {}

    writer = csv.writer(sys.stdout)
    writer.writerow(['key', 'type', 'flags', 'default', 'source', 'usages'])

    for reg in registrations:
        usages = usage_map.get(reg['key'], '')
        writer.writerow(
            [reg['key'], reg['type'], reg['flags'], reg['default'], reg['source'], usages],
        )

    # Summary to stderr
    source_counts = Counter(reg['source'] for reg in registrations)

    print(file=sys.stderr)
    print(f"sentry SHA: {get_sha(args.sentry)}", file=sys.stderr)
    print(f"getsentry SHA: {get_sha(args.getsentry)}", file=sys.stderr)
    print(f"total options: {len(registrations)}", file=sys.stderr)
    print(file=sys.stderr)
    print('registrations per file:', file=sys.stderr)
    for source, count in source_counts.most_common():
        print(f"  {count:4d}  {source}", file=sys.stderr)


if __name__ == '__main__':
    main()
