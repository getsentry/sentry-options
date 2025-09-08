from __future__ import annotations

import argparse
import collections
import json
import os.path
from collections.abc import Sequence
from typing import NamedTuple

import yaml

from sentry_options.api import _groups
from sentry_options.validate_type import is_type_valid


class _FileData(NamedTuple):
    fname: str
    data: dict[str, object]


def _merged(by_file: list[_FileData]) -> dict[str, object]:
    ret = {}
    for _, values in by_file:
        ret.update(values)
    return ret


def _validate(group: str, fname: str) -> dict[str, object]:
    with open(fname) as f:
        data = yaml.safe_load(f)

    if not isinstance(data, dict):
        raise SystemExit(f'expected top-level mapping: {fname}')
    elif tuple(data) != ('options',):
        raise SystemExit(f'expected top-level `options:`: {fname}')
    elif not isinstance(data['options'], dict):
        raise SystemExit(f'expected top-level `options:` dict: {fname}')

    group_types = _groups[group]

    for k, v in data['options'].items():
        if k not in group_types:
            raise SystemExit(
                f'unknown option `{k}` (in group `{group}`): {fname}',
            )

        tp = group_types[k].tp
        if not is_type_valid(tp, v):
            raise SystemExit(
                f'incorrect option type (`[{group}]{k}`): '
                f'expected `{tp}` got `{v!r}`: {fname}',
            )

    return data['options']


def _check_overlap(grouped: dict[str, dict[str, list[_FileData]]]) -> None:
    for group, targets in grouped.items():
        for target, by_file in targets.items():
            for i, (fname, values) in enumerate(by_file):
                for (other_fname, other_values) in by_file[i + 1:]:
                    overlap = sorted(values.keys() & other_values.keys())
                    if overlap:
                        raise SystemExit(
                            f'key(s) {overlap} in {group}/{target} '
                            f'are defined by both {fname} and {other_fname}',
                        )


def _load(root: str) -> dict[str, dict[str, list[_FileData]]]:
    grouped: dict[str, dict[str, list[_FileData]]]
    grouped = collections.defaultdict(lambda: collections.defaultdict(list))

    for pth, _, fnames in os.walk(root):
        for fname in fnames:
            joined = os.path.join(pth, fname)
            if joined.endswith('.yml'):
                raise SystemExit(f'expected .yaml file: {joined}')
            elif joined.endswith('.yaml'):
                rel = os.path.relpath(joined, root)
                parts = rel.split(os.sep)
                if len(parts) != 3:
                    raise SystemExit(
                        f'expected {{group}}/{{target}}/{{file}}.yaml: '
                        f'{joined}',
                    )
                group, target, _ = parts
                if group not in _groups:
                    raise SystemExit(f'unexpected group: {group}: {joined}')

                validated = _validate(group, joined)
                grouped[group][target].append(_FileData(joined, validated))

    for targets in grouped.values():
        for by_file in targets.values():
            by_file.sort()
    _check_overlap(grouped)
    return grouped


def main(argv: Sequence[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        '--root',
        required=True,
        help='sentry-options root (usually ./sentry-options)',
    )
    parser.add_argument(
        '--out',
        required=True,
        help='output directory',
    )
    args = parser.parse_args(argv)

    # expected structure:
    # {group}/default/*.yaml  # defaults for all targets
    # {group}/{target}/*.yaml  # per-target values (ex: us)

    if os.path.exists(args.out):
        raise SystemExit(f'output already exists: {args.out}')

    grouped = _load(args.root)

    outputs = []
    for group, targets in grouped.items():
        defaults = _merged(targets.pop('default', []))
        for target, by_file in targets.items():
            data = {'options': {**defaults, **_merged(by_file)}}
            text = json.dumps(data, separators=(',', ':'))
            outputs.append((f'sentry-options-{group}-{target}.json', text))

    os.makedirs(args.out, exist_ok=True)
    for fname, text in outputs:
        with open(os.path.join(args.out, fname), 'w', encoding='UTF-8') as f:
            f.write(text)

    return 0


if __name__ == '__main__':
    raise SystemExit(main())
