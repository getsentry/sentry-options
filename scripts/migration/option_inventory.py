#!/usr/bin/env python3
"""
Extracts all registered options from sentry and getsentry, then classifies
each option's safety for deletion using two complementary passes:

  1. AST pass (Python files only): finds options.get/set/delete("literal-key")
     calls and options.get(variable) dynamic-access calls.
  2. String pass (all file types, full repo): catches non-Python consumers
     (YAML, JS, shell, Helm charts) and aliased/indirect Python access
     that AST misses.

Also cross-references options_mapper from sentry's initializer so that
options promoted to Django settings are never silently classified as unused.

Usage:
    python scripts/migration/option_inventory.py \\
        --sentry ~/code/sentry \\
        --getsentry ~/code/getsentry \\
        > inventory.csv

Safety column values:
    NEVER_DELETE   - static usage found, in options_mapper, or FLAG_REQUIRED
    INVESTIGATE    - no static usage found, but dynamic options.get(variable)
                     calls exist somewhere in the codebase — manual review required
    SAFE_CANDIDATE - no evidence of use anywhere; still requires human confirmation

The summary (SHAs, counts, dynamic-access files) goes to stderr so it
appears in the terminal even when stdout is redirected to a file.
"""
from __future__ import annotations

import argparse
import ast
import concurrent.futures
import csv
import html as _html
import json
import os
import re
import subprocess
import sys
import textwrap
from collections import Counter
from dataclasses import dataclass, field

CHUNK_SIZE = 100


# ---------------------------------------------------------------------------
# Registration extraction
# ---------------------------------------------------------------------------

def _extract_block(content: str, start: int) -> str:
    """Return the full text of the call starting at `start`, matching parens."""
    depth = 0
    for i in range(start, len(content)):
        if content[i] == '(':
            depth += 1
        elif content[i] == ')':
            depth -= 1
            if depth == 0:
                return content[start:i + 1]
    return content[start:]


def extract_registrations(file_path: str) -> list[dict]:
    """Extract all register() calls from a Python file."""
    with open(file_path) as f:
        content = f.read()
    lines = content.split('\n')

    results = []
    pattern = re.compile(r'(?:options\.)?register\(\s*\n?\s*"([^"]+)"', re.MULTILINE)
    for match in pattern.finditer(content):
        lineno = content[:match.start()].count('\n') + 1  # 1-indexed
        block = _extract_block(content, match.start())
        key = match.group(1)

        # Collect comment lines immediately above the register() call (the description).
        comment_lines: list[str] = []
        i = lineno - 2  # 0-indexed line just above
        while i >= 0 and lines[i].strip().startswith('#'):
            comment_lines.insert(0, lines[i])
            i -= 1

        declaration = '\n'.join(comment_lines + block.split('\n'))

        type_m = re.search(r'type=(\w+)', block)
        flags_m = re.search(r'flags=([\w\s|]+?)(?:,|\))', block)
        default_m = re.search(r'default=([^,\)]+)', block)

        results.append({
            'key': key,
            'type': type_m.group(1) if type_m else 'inferred',
            'flags': flags_m.group(1).strip() if flags_m else 'DEFAULT_FLAGS',
            'default': default_m.group(1).strip() if default_m else '',
            'source': file_path,
            'lineno': lineno,
            'declaration': declaration,
        })
    return results


# ---------------------------------------------------------------------------
# options_mapper extraction
# ---------------------------------------------------------------------------

def _collect_mapper_from_ast(tree: ast.AST) -> dict[str, str]:
    """
    Walk an AST and collect all entries from options_mapper assignments and
    options_mapper.update(...) calls.  Handles both the initial dict literal
    and the conditional .update() block in initializer.py.
    """
    mapping: dict[str, str] = {}

    for node in ast.walk(tree):
        # options_mapper = { "key": "SETTING", ... }
        if (
            isinstance(node, ast.Assign)
            and any(isinstance(t, ast.Name) and t.id == 'options_mapper' for t in node.targets)
            and isinstance(node.value, ast.Dict)
        ):
            for k, v in zip(node.value.keys, node.value.values):
                if isinstance(k, ast.Constant) and isinstance(v, ast.Constant):
                    mapping[str(k.value)] = str(v.value)

        # options_mapper.update({ "key": "SETTING", ... })
        if (
            isinstance(node, ast.Call)
            and isinstance(node.func, ast.Attribute)
            and node.func.attr == 'update'
            and isinstance(node.func.value, ast.Name)
            and node.func.value.id == 'options_mapper'
        ):
            for arg in node.args:
                if isinstance(arg, ast.Dict):
                    for k, v in zip(arg.keys, arg.values):
                        if isinstance(k, ast.Constant) and isinstance(v, ast.Constant):
                            mapping[str(k.value)] = str(v.value)
            for kw in node.keywords:
                if isinstance(kw.value, ast.Constant):
                    mapping[kw.arg] = str(kw.value.value)

    return mapping


def find_options_mapper(sentry_path: str) -> dict[str, str]:
    """
    Return {option_key: django_setting_name} by AST-parsing all Python files
    that reference options_mapper in sentry's source tree.
    """
    try:
        result = subprocess.run(
            ['rg', '-l', '--type', 'py', r'options_mapper', sentry_path],
            capture_output=True, text=True, timeout=15,
        )
    except (subprocess.TimeoutExpired, FileNotFoundError):
        print('WARNING: could not search for options_mapper', file=sys.stderr)
        return {}

    mapping: dict[str, str] = {}
    for candidate in result.stdout.strip().split('\n'):
        if not candidate:
            continue
        try:
            with open(candidate) as f:
                content = f.read()
            tree = ast.parse(content, filename=candidate)
            mapping.update(_collect_mapper_from_ast(tree))
        except Exception as exc:
            print(f'WARNING: could not parse {candidate}: {exc}', file=sys.stderr)

    return mapping


# ---------------------------------------------------------------------------
# AST pass: find options.get/set/delete() calls in Python files
# ---------------------------------------------------------------------------

_OPTIONS_CALL_ATTRS = frozenset({'get', 'set', 'delete', 'isset'})


@dataclass
class ASTScanResult:
    # key -> set of file paths where options.get("key") was found
    literal_usages: dict[str, set[str]] = field(default_factory=dict)
    # files containing options.get(variable) — key cannot be determined statically
    dynamic_access_files: set[str] = field(default_factory=set)


# ---------------------------------------------------------------------------
# Dynamic file attribution: bound what keys each dynamic-access file can reach
# ---------------------------------------------------------------------------

@dataclass
class FileAttribution:
    # Key prefixes this file can reach (e.g. "backpressure.high_watermarks.")
    bounded_prefixes: list[str] = field(default_factory=list)
    # Key suffixes this file can reach (e.g. ".disallow-new-enrollment")
    bounded_suffixes: list[str] = field(default_factory=list)
    # Exact keys this file can reach (enumerated from a static registry)
    bounded_keys: set[str] = field(default_factory=set)
    # File reads options.all() / options.filter() — management plane, not app use
    is_admin_plane: bool = False
    # File has dynamic calls we genuinely cannot attribute to a bounded set
    unresolved: bool = False


def _fstring_prefix(node: ast.JoinedStr) -> str | None:
    """Return the leading literal string of an f-string, or None."""
    if node.values and isinstance(node.values[0], ast.Constant):
        v = node.values[0].value
        if isinstance(v, str):
            return v
    return None


def _fstring_suffix(node: ast.JoinedStr) -> str | None:
    """
    Return the trailing literal string of an f-string, or None.
    Handles patterns like f"{self.interface_id}.disallow-new-enrollment"
    where the suffix is the fixed part.
    """
    if node.values and isinstance(node.values[-1], ast.Constant):
        v = node.values[-1].value
        if isinstance(v, str):
            return v
    return None


def _class_str_attrs(tree: ast.AST) -> dict[str, str]:
    """Return {attr_name: string_value} for all class-level string assignments."""
    attrs: dict[str, str] = {}
    for node in ast.walk(tree):
        if not isinstance(node, ast.ClassDef):
            continue
        for stmt in node.body:
            if (
                isinstance(stmt, ast.Assign)
                and len(stmt.targets) == 1
                and isinstance(stmt.targets[0], ast.Name)
                and isinstance(stmt.value, ast.Constant)
                and isinstance(stmt.value.value, str)
            ):
                attrs[stmt.targets[0].id] = stmt.value.value
    return attrs


def _enumerate_static_dict(tree: ast.AST, var_name: str) -> set[str] | None:
    """
    Find `var_name = {"key": ..., ...}` in the module and return the string keys,
    or None if not found / not a plain dict literal.
    """
    for node in ast.walk(tree):
        if not isinstance(node, ast.Assign):
            continue
        if not any(isinstance(t, ast.Name) and t.id == var_name for t in node.targets):
            continue
        if not isinstance(node.value, ast.Dict):
            continue
        keys: set[str] = set()
        for k in node.value.keys:
            if isinstance(k, ast.Constant) and isinstance(k.value, str):
                keys.add(k.value)
        return keys
    return None


def _attribute_dynamic_file(file_path: str) -> FileAttribution:
    attr = FileAttribution()
    try:
        with open(file_path, errors='replace') as f:
            source = f.read()
        tree = ast.parse(source, filename=file_path)
    except Exception:
        attr.unresolved = True
        return attr

    # Collect class-level string constants for self.attr_name resolution.
    class_str = _class_str_attrs(tree)

    # Try to enumerate a static registry dict used for killswitch-style access.
    # Convention: if the file has ALL_*_OPTIONS = {...} we treat those keys as bounded.
    for node in ast.walk(tree):
        if (
            isinstance(node, ast.Assign)
            and any(
                isinstance(t, ast.Name) and t.id.startswith('ALL_') and t.id.endswith('OPTIONS')
                for t in node.targets
            )
            and isinstance(node.value, ast.Dict)
        ):
            for k in node.value.keys:
                if isinstance(k, ast.Constant) and isinstance(k.value, str):
                    attr.bounded_keys.add(k.value)

    for func_node in ast.walk(tree):
        if not isinstance(func_node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue

        # Parameters of this function — keys passed as params are resolved at
        # the call site (already caught by the AST literal pass on callers).
        param_names = {a.arg for a in func_node.args.args + func_node.args.posonlyargs + func_node.args.kwonlyargs}

        # Simple name→value map for single-assignment variables in this scope.
        local_vals: dict[str, ast.expr] = {}
        for stmt in ast.walk(func_node):
            if isinstance(stmt, ast.Assign) and len(stmt.targets) == 1:
                t = stmt.targets[0]
                if isinstance(t, ast.Name):
                    local_vals[t.id] = stmt.value

        for node in ast.walk(func_node):
            if not isinstance(node, ast.Call):
                continue
            func = node.func
            if not (
                isinstance(func, ast.Attribute)
                and isinstance(func.value, ast.Name)
                and func.value.id == 'options'
            ):
                continue

            # options.all() / options.filter() → management plane
            if func.attr in ('all', 'filter'):
                attr.is_admin_plane = True
                continue

            if func.attr not in _OPTIONS_CALL_ATTRS or not node.args:
                continue

            arg = node.args[0]

            # Literal key — already handled by main AST pass.
            if isinstance(arg, ast.Constant) and isinstance(arg.value, str):
                continue

            # F-string directly: options.get(f"prefix.{var}") or f"{var}.suffix"
            if isinstance(arg, ast.JoinedStr):
                prefix = _fstring_prefix(arg)
                if prefix:
                    attr.bounded_prefixes.append(prefix)
                    continue
                suffix = _fstring_suffix(arg)
                if suffix:
                    attr.bounded_suffixes.append(suffix)
                    continue
                attr.unresolved = True
                continue

            # Variable name: resolve it.
            if isinstance(arg, ast.Name):
                name = arg.id

                # Parameter — key comes from caller, already caught at call sites.
                if name in param_names:
                    continue

                # Locally assigned f-string: model_key = f"prefix.{x}" or f"{x}.suffix"
                if name in local_vals:
                    val = local_vals[name]
                    if isinstance(val, ast.JoinedStr):
                        prefix = _fstring_prefix(val)
                        if prefix:
                            attr.bounded_prefixes.append(prefix)
                        else:
                            suffix = _fstring_suffix(val)
                            if suffix:
                                attr.bounded_suffixes.append(suffix)
                            else:
                                attr.unresolved = True
                    elif isinstance(val, ast.Constant) and isinstance(val.value, str):
                        pass  # literal, already caught
                    else:
                        attr.unresolved = True
                    continue

                # Bounded by an enumerated static dict (e.g. killswitch registry).
                if attr.bounded_keys:
                    continue  # assume this name draws from the registry

                attr.unresolved = True
                continue

            # Attribute access: self.enabled_option_name
            if isinstance(arg, ast.Attribute) and isinstance(arg.value, ast.Name):
                attr_name = arg.attr
                if attr_name in class_str:
                    attr.bounded_keys.add(class_str[attr_name])
                    continue
                attr.unresolved = True
                continue

            attr.unresolved = True

    # Module-level options.all()/filter() (outside functions)
    for node in ast.walk(tree):
        if (
            isinstance(node, ast.Call)
            and isinstance(node.func, ast.Attribute)
            and isinstance(node.func.value, ast.Name)
            and node.func.value.id == 'options'
            and node.func.attr in ('all', 'filter')
        ):
            attr.is_admin_plane = True

    return attr


def attribute_dynamic_files(dynamic_files: set[str]) -> dict[str, FileAttribution]:
    return {f: _attribute_dynamic_file(f) for f in dynamic_files}


_DEPRECATION_MARKERS = ('# unused', '# todo: remove', '# no longer', '# deprecated', '# dead')


def compute_hints(reg: dict, safety: str, all_safety: dict[str, str]) -> list[str]:
    hints: list[str] = []
    decl_lower = reg.get('declaration', '').lower()
    flags = reg.get('flags', '')
    if any(m in decl_lower for m in _DEPRECATION_MARKERS):
        hints.append('deprecation_note')
    # Secrets should never live in a git-tracked file-based config system.
    if 'FLAG_CREDENTIAL' in flags:
        hints.append('credential_flag')
    # Immutable options can't change at runtime — they're better as Django settings.
    if 'FLAG_IMMUTABLE' in flags:
        hints.append('immutable_flag')
    # Namespace peer analysis: all other options in the same top-level namespace
    # are NEVER_DELETE, but this one isn't — it stands out.
    if safety != 'NEVER_DELETE':
        ns = reg['key'].split('.')[0]
        peer_safeties = [v for k, v in all_safety.items() if k.split('.')[0] == ns and k != reg['key']]
        if peer_safeties and all(s == 'NEVER_DELETE' for s in peer_safeties):
            hints.append('all_peers_used')
    return hints


def _scan_python_file(file_path: str, known_keys: set[str], result: ASTScanResult) -> None:
    """AST-scan one Python file for options accessor calls."""
    try:
        with open(file_path, errors='replace') as f:
            source = f.read()
        tree = ast.parse(source, filename=file_path)
    except (SyntaxError, OSError, ValueError):
        return

    for node in ast.walk(tree):
        if not isinstance(node, ast.Call):
            continue
        func = node.func
        # Match `options.get(...)` where `options` is any local name
        if not (
            isinstance(func, ast.Attribute)
            and func.attr in _OPTIONS_CALL_ATTRS
            and isinstance(func.value, ast.Name)
            and func.value.id == 'options'
        ):
            continue
        if not node.args:
            continue
        first_arg = node.args[0]
        if isinstance(first_arg, ast.Constant) and isinstance(first_arg.value, str):
            key = first_arg.value
            if key in known_keys:
                result.literal_usages.setdefault(key, set()).add(file_path)
        else:
            result.dynamic_access_files.add(file_path)


def ast_scan_repos(
    known_keys: set[str],
    search_dirs: list[str],
    exclude_files: set[str],
) -> ASTScanResult:
    """Run AST scan over all Python files in search_dirs."""
    result = ASTScanResult()
    for search_dir in search_dirs:
        try:
            rg = subprocess.run(
                ['rg', '-l', '--type', 'py', r'options\.(?:get|set|delete|isset)\s*\(', search_dir],
                capture_output=True, text=True, timeout=120,
            )
        except (subprocess.TimeoutExpired, FileNotFoundError):
            continue
        for filepath in rg.stdout.strip().split('\n'):
            if filepath and filepath not in exclude_files:
                _scan_python_file(filepath, known_keys, result)
    return result


# ---------------------------------------------------------------------------
# String pass: all file types, full repos
# ---------------------------------------------------------------------------

def string_scan_repos(
    keys: list[str],
    search_dirs: list[str],
    exclude_files: set[str],
) -> dict[str, set[str]]:
    """
    Search all file types across full repo dirs for literal key strings.
    Returns {key: set_of_files}.  Over-counts intentionally — for paranoid mode,
    false positives (saying used when not) are the safe direction.
    """
    escaped = [re.escape(k) for k in keys]
    usage_map: dict[str, set[str]] = {k: set() for k in keys}

    for search_dir in search_dirs:
        for i in range(0, len(escaped), CHUNK_SIZE):
            chunk_escaped = escaped[i:i + CHUNK_SIZE]
            chunk_keys = keys[i:i + CHUNK_SIZE]
            pattern = '|'.join(chunk_escaped)
            try:
                rg = subprocess.run(
                    ['rg', '--no-heading', '-l', pattern, search_dir],
                    capture_output=True, text=True, timeout=300,
                )
            except (subprocess.TimeoutExpired, FileNotFoundError):
                continue
            for filepath in rg.stdout.strip().split('\n'):
                if not filepath or filepath in exclude_files:
                    continue
                try:
                    with open(filepath, errors='replace') as f:
                        content = f.read()
                    for key in chunk_keys:
                        if key in content:
                            usage_map[key].add(filepath)
                except OSError:
                    pass

    return usage_map


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

_SAFETY_ORDER = {'SAFE_CANDIDATE': 0, 'INVESTIGATE': 1, 'NEVER_DELETE': 2}


@dataclass
class Palette:
    reset:  str = ''
    bold:   str = ''
    dim:    str = ''
    decl:   str = ''  # code-snippet lines — darker than regular dim
    red:    str = ''
    yellow: str = ''
    cyan:   str = ''

    def for_status(self, status: str) -> str:
        return {'NEVER_DELETE': self.dim, 'INVESTIGATE': self.yellow, 'SAFE_CANDIDATE': self.red}.get(status, '')

    def for_found(self, found: str) -> str:
        return self.red if found == 'none' else self.dim


def _make_palette(enabled: bool) -> Palette:
    if not enabled:
        return Palette()
    return Palette(
        reset='\033[0m',
        bold='\033[1m',
        dim='\033[2m',
        decl='\033[90m',   # dark grey — visibly dimmer than regular text
        red='\033[31m',
        yellow='\033[33m',
        cyan='\033[36m',
    )


_SEP = '─' * 72


def _print_entry(
    reg: dict,
    safety: str,
    found: str,
    usages: list[str],
    django_setting: str,
    hints: list[str],
    p: Palette,
    stream,
) -> None:
    sc = p.for_status(safety)
    fc = p.for_found(found)
    print(_SEP, file=stream)
    hints_str = f'  {p.yellow}[{", ".join(hints)}]{p.reset}' if hints else ''
    print(
        f'{sc}{p.bold}{safety:<14}{p.reset}  '
        f'{p.bold}{reg["key"]}{p.reset}  '
        f'{fc}{found}{p.reset}'
        f'{hints_str}',
        file=stream,
    )
    loc = f'{reg["source"]}:{reg["lineno"]}'
    if django_setting:
        loc += f'  →  settings.{django_setting}'
    print(f'{p.dim}{loc}{p.reset}', file=stream)
    print(file=stream)
    for line in reg['declaration'].split('\n'):
        print(f'  {p.decl}{line}{p.reset}', file=stream)
    print(file=stream)
    if usages:
        print(f'  {p.cyan}usages ({len(usages)}):{p.reset}', file=stream)
        for f in usages:
            print(f'    {p.cyan}{f}{p.reset}', file=stream)
    else:
        print(f'  {p.red}no usages found{p.reset}', file=stream)
    print(file=stream)


def print_plan_report(
    registrations: list[dict],
    merged_usages: dict[str, set[str]],
    options_mapper: dict[str, str],
    found_by_map: dict[str, str],
    safety_map: dict[str, str],
    hints_map: dict[str, list[str]],
    file_attributions: dict[str, FileAttribution],
    first_commits: dict[str, dict | None],
    roots: list[str],
    sentry_sha: str,
    getsentry_sha: str,
    out,
) -> None:
    candidates = [
        r for r in registrations
        if safety_map[r['key']] in ('INVESTIGATE', 'SAFE_CANDIDATE')
    ]
    candidates.sort(key=lambda r: (hints_map[r['key']] == [], r['key']))  # hints first

    unresolved_files = sorted(
        shorten(f, roots)
        for f, a in file_attributions.items()
        if a.unresolved and not a.is_admin_plane
    )
    admin_files = sorted(
        shorten(f, roots)
        for f, a in file_attributions.items()
        if a.is_admin_plane
    )

    def w(s: str = '') -> None:
        print(s, file=out)

    w('# Options Cleanup Plan')
    w()
    w(f'Generated from `sentry@{sentry_sha[:9]}` · `getsentry@{getsentry_sha[:9]}`.')
    w(f'{len(candidates)} candidate options require verification before any deletions.')
    w()
    w('---')
    w()
    w('## How to use this plan')
    w()
    w(textwrap.dedent('''\
        This file is designed to be worked through by a coding agent (or a human)
        option-by-option. Each section below corresponds to one candidate option.
        Work top-to-bottom. Options with hints (`deprecation_note`, `all_peers_used`)
        appear first — they are higher-confidence candidates.

        **Do not delete an option until all checklist items are ticked.**
        A missed dynamic usage will cause a `KeyError` or silent `None` return at runtime.

        ### Verification workflow

        For each option, perform the following checks in order:

        #### 1. Re-run the static search (takes seconds)
        ```bash
        rg -l '<option-key>' ~/dev/sentry ~/dev/getsentry
        ```
        If any file other than the registration file is returned, treat the option as
        `NEVER_DELETE` and skip it. The inventory may be stale relative to HEAD.

        #### 2. Check dynamic access files
        The files listed under "Unresolved dynamic-access files" below contain
        `options.get(variable)` calls whose keys cannot be determined statically.
        Before deleting an option, grep each of those files for the option's *prefix*
        (everything before the first `.`) to check whether the file's access pattern
        could reach it:
        ```bash
        rg '<key-prefix>' <dynamic-file>
        ```
        If the prefix appears in a meaningful context (f-string template, dict key,
        class attribute), do not delete the option without domain-owner confirmation.

        #### 3. Check git history in both repos
        ```bash
        git -C ~/dev/sentry  log --all -S '<option-key>' --oneline | head -10
        git -C ~/dev/getsentry log --all -S '<option-key>' --oneline | head -10
        ```
        A recent commit removing an `options.get(...)` call is strong evidence the
        option is dead. An old commit *adding* the registration with no paired usage
        commit is suspicious.

        #### 4. Check the production database (human step — do not automate)
        This cannot be done from source code. Ask an engineer with DB access to run:
        ```sql
        SELECT key, value, last_updated_by FROM options_option WHERE key = '<option-key>';
        ```
        If a non-default value has ever been set, treat the option as `NEVER_DELETE`.

        #### 5. Confirm with domain owner
        Options in namespaces owned by specific teams (`seer.*`, `relay.*`,
        `billing.*`, `metrics.*`) should be confirmed safe with that team before
        deletion, even if static analysis shows no usages.

        #### 6. Delete
        Once all checks pass:
        - Remove the `register(...)` block from its source file.
        - Search for the key in `configs/` YAML value files and remove any entries.
        - Run `cargo test` and `pytest tests/` to confirm nothing broke.
        - Commit with message: `chore(options): remove unused option <key>`
    ''').rstrip())
    w()
    w('---')
    w()
    w('## Unresolved dynamic-access files')
    w()
    w('These files contain `options.get(variable)` calls that could not be '
      'statically attributed to a specific key or namespace. Check the relevant '
      'ones before deleting options in related namespaces.')
    w()
    for f in unresolved_files:
        w(f'- `{f}`')
    if admin_files:
        w()
        w('The following are management-plane files (admin APIs / CLI) that iterate '
          'all options — they do not represent application use of specific options:')
        w()
        for f in admin_files:
            w(f'- `{f}`')
    w()
    w('---')
    w()
    w('## Candidate options')
    w()
    w(f'<!-- {len(candidates)} options to review -->')
    w()

    for reg in candidates:
        key = reg['key']
        safety = safety_map[key]
        found = found_by_map[key]
        hints = hints_map.get(key, [])
        usages = sorted(shorten(f, roots) for f in merged_usages.get(key, set()))
        django_setting = options_mapper.get(key, '')

        w(f'### `{key}`')
        w()
        meta = [
            f'**Source**: `{reg["source"]}:{reg["lineno"]}`',
            f'**Status**: `{safety}`',
            f'**found\\_by**: `{found}`',
        ]
        if django_setting:
            meta.append(f'**Django setting**: `settings.{django_setting}`')
        if hints:
            meta.append(f'**Hints**: {", ".join(f"`{h}`" for h in hints)}')
        w('  \n'.join(meta))
        w()
        w('```python')
        for line in reg['declaration'].split('\n'):
            w(line)
        w('```')
        w()
        if usages:
            w(f'**Usages found ({len(usages)})**:')
            for u in usages:
                w(f'- `{u}`')
            w()
        else:
            w('*No usages found by static analysis.*')
            w()
        commit = first_commits.get(key)
        if commit:
            gh = commit.get('github_url')
            sha_md = f'[`{commit["sha"]}`]({gh})' if gh else f'`{commit["sha"]}`'
            w(f'**First introduced**: {sha_md} · {commit["author"]} · {commit["date"]}')
            w(f'> {commit["message"]}')
            w()
            if commit['diff_snippet']:
                w('<details><summary>diff</summary>')
                w()
                w('```diff')
                w(commit['diff_snippet'])
                w('```')
                w()
                w('</details>')
                w()
        w('**Verification checklist**')
        w()
        w('- [ ] `rg` confirms no new usages since inventory was generated')
        w('- [ ] Dynamic-access files checked for key prefix')
        w('- [ ] Git history reviewed in both repos')
        w('- [ ] Production DB confirmed no non-default value (human step)')
        if key.split('.')[0] in ('seer', 'relay', 'billing', 'metrics', 'getsentry'):
            w('- [ ] Domain owner confirmed safe to delete')
        w('- [ ] `register()` block removed from source')
        w('- [ ] Key removed from any `configs/` YAML value files')
        w('- [ ] Tests pass')
        w()


def print_all_report(
    registrations: list[dict],
    merged_usages: dict[str, set[str]],
    options_mapper: dict[str, str],
    found_by_map: dict[str, str],
    safety_map: dict[str, str],
    hints_map: dict[str, list[str]],
    roots: list[str],
    p: Palette,
) -> None:
    sorted_regs = sorted(
        registrations,
        key=lambda r: (_SAFETY_ORDER.get(safety_map[r['key']], 99), r['key']),
    )
    for reg in sorted_regs:
        key = reg['key']
        usages = sorted(shorten(f, roots) for f in merged_usages.get(key, set()))
        _print_entry(
            reg, safety_map[key], found_by_map[key],
            usages, options_mapper.get(key, ''),
            hints_map.get(key, []), p, sys.stdout,
        )


def print_html_report(
    registrations: list[dict],
    merged_usages: dict[str, set[str]],
    options_mapper: dict[str, str],
    found_by_map: dict[str, str],
    safety_map: dict[str, str],
    hints_map: dict[str, list[str]],
    first_commits: dict[str, dict | None],
    roots: list[str],
    sentry_sha: str,
    getsentry_sha: str,
    *,
    active_db_keys: set[str] | None = None,
    active_disk_keys: set[str] | None = None,
    active_all_keys: set[str] | None = None,
    active_unknown_db: set[str] | None = None,
) -> None:
    active_db_keys    = active_db_keys    or set()
    active_disk_keys  = active_disk_keys  or set()
    active_all_keys   = active_all_keys   or set()
    active_unknown_db = active_unknown_db or set()
    def esc(s: str) -> str:
        return _html.escape(str(s))

    safety_counts = Counter(safety_map.values())
    # total is computed after runtime cards are known; placeholder updated in template
    total = len(registrations)  # updated below once runtime_keys is known
    sorted_regs = sorted(
        registrations,
        key=lambda r: (_SAFETY_ORDER.get(safety_map[r['key']], 99), r['key']),
    )

    cards: list[str] = []
    for reg in sorted_regs:
        key = reg['key']
        safety = safety_map[key]
        found = found_by_map[key]
        usages = sorted(shorten(f, roots) for f in merged_usages.get(key, set()))
        django_setting = options_mapper.get(key, '')

        sc = {'INVESTIGATE': 'investigate', 'SAFE_CANDIDATE': 'safe', 'NEVER_DELETE': 'never'}[safety]
        open_attr = 'open' if safety in ('INVESTIGATE', 'SAFE_CANDIDATE') else ''
        found_cls = 'found-none' if found == 'none' else 'found-hit'
        hints = hints_map.get(key, [])

        loc = f'{esc(reg["source"])}:{reg["lineno"]}'
        if django_setting:
            loc += f' <span class="django-tag">→ settings.{esc(django_setting)}</span>'

        hints_html = ''.join(f'<span class="hint-tag">{esc(h)}</span>' for h in hints)
        db_html = '<span class="db-tag">DB value</span>' if key in active_db_keys else ''

        if usages:
            items = ''.join(f'<span class="usage-file">{esc(f)}</span>' for f in usages)
            usages_html = f'<div class="usages-section"><div class="usages-label">usages ({len(usages)})</div>{items}</div>'
        else:
            usages_html = '<div class="no-usages">no usages found</div>'

        # Collapsed first-introduction commit (INVESTIGATE only).
        commit_html = ''
        commit = first_commits.get(key)
        if commit:
            diff_html = _diff_to_html(commit['diff_snippet']) if commit['diff_snippet'] else '(diff not available)'
            gh = commit.get('github_url')
            sha_html = (
                f'<a class="git-sha" href="{esc(gh)}" target="_blank" rel="noopener">{esc(commit["sha"])}</a>'
                if gh else
                f'<span class="git-sha">{esc(commit["sha"])}</span>'
            )
            commit_html = (
                f'<details class="git-intro">'
                f'<summary class="git-intro-summary">'
                f'introduced '
                f'{sha_html}'
                f'<span>{esc(commit["author"])}</span>'
                f'<span>{esc(commit["date"])}</span>'
                f'<span class="git-msg">{esc(commit["message"])}</span>'
                f'</summary>'
                f'<pre class="diff">{diff_html}</pre>'
                f'</details>'
            )

        cards.append(f'''\
<div class="option option-{sc}" data-key="{esc(key)}" data-status="{safety}">
<details {open_attr}><summary class="option-summary">
  <span class="badge badge-{sc}">{esc(safety)}</span>
  <span class="option-key">{esc(key)}</span>
  {db_html}{hints_html}<span class="found-tag {found_cls}">{esc(found)}</span>
</summary><div class="option-body">
  <div class="option-location">{loc}</div>
  <pre class="decl">{esc(reg["declaration"])}</pre>
  {usages_html}
  {commit_html}
</div></details></div>''')

    # Runtime-only cards (not in static scan, auto-registered at runtime)
    static_keys = {r['key'] for r in registrations}
    runtime_keys = sorted(active_all_keys - static_keys)
    for key in runtime_keys:
        sub = 'feature' if key.startswith('feature.') else ('dynamic' if key.startswith('dynamic.') else 'other')
        db_html = '<span class="db-tag">DB value</span>' if key in active_db_keys else ''
        disk_html = '<span class="db-tag disk">disk</span>' if key in active_disk_keys else ''
        cards.append(f'''\
<div class="option option-runtime" data-key="{esc(key)}" data-status="RUNTIME_ONLY">
<details><summary class="option-summary">
  <span class="badge badge-runtime">runtime/{esc(sub)}</span>
  <span class="option-key">{esc(key)}</span>
  {db_html}{disk_html}
</summary><div class="option-body">
  <div class="option-location">auto-registered at runtime · not in static scan</div>
</div></details></div>''')

    # db-only cards (have DB value, found by neither scan)
    for key in sorted(active_unknown_db):
        cards.append(f'''\
<div class="option option-runtime" data-key="{esc(key)}" data-status="DB_ONLY">
<details><summary class="option-summary">
  <span class="badge badge-runtime">db-only</span>
  <span class="option-key">{esc(key)}</span>
  <span class="db-tag">DB value</span>
</summary><div class="option-body">
  <div class="option-location">found in database · not in static scan or runtime registration</div>
</div></details></div>''')

    n_inv     = safety_counts.get('INVESTIGATE', 0)
    n_safe    = safety_counts.get('SAFE_CANDIDATE', 0)
    n_nev     = safety_counts.get('NEVER_DELETE', 0)
    n_runtime = len(runtime_keys) + len(active_unknown_db)
    total     = len(registrations) + n_runtime

    print(f'''\
<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Options Inventory</title>
<style>
*,*::before,*::after{{box-sizing:border-box;margin:0;padding:0}}
body{{font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;background:#fff;color:#24292f;font-size:14px;line-height:1.5}}
/* ── topbar ── */
.topbar{{position:sticky;top:0;z-index:100;background:#fff;border-bottom:1px solid #d0d7de;padding:10px 24px;display:flex;gap:10px;align-items:center;flex-wrap:wrap}}
.topbar-title{{font-size:14px;font-weight:600;color:#24292f;white-space:nowrap}}
.topbar-sha{{font-size:11px;color:#8c959f;font-family:monospace;white-space:nowrap}}
/* stat pills — clickable shortcuts */
.stat{{font-size:12px;font-weight:500;padding:2px 8px;border-radius:10px;cursor:pointer;white-space:nowrap;border:1px solid transparent}}
.stat-investigate{{background:#fff8c5;color:#7d4e00;border-color:#e3b341}}
.stat-safe{{background:#ffebe9;color:#cf222e;border-color:#f7928a}}
.stat-never{{background:#f6f8fa;color:#656d76;border-color:#d0d7de}}
/* ── controls ── */
.controls{{display:flex;gap:6px;align-items:center;flex-wrap:wrap;margin-left:auto}}
.filter-btn{{padding:4px 10px;border:1px solid #d0d7de;border-radius:6px;background:#f6f8fa;color:#24292f;font-size:12px;cursor:pointer;font-weight:500;transition:background .1s}}
.filter-btn:hover{{background:#eaeef2}}
.filter-btn.active{{background:#0969da;color:#fff;border-color:#0969da}}
.search-box{{padding:4px 10px;border:1px solid #d0d7de;border-radius:6px;font-size:13px;outline:none;width:200px;color:#24292f}}
.search-box:focus{{border-color:#0969da;box-shadow:0 0 0 3px rgba(9,105,218,.1)}}
.vis-count{{font-size:12px;color:#8c959f;white-space:nowrap}}
.text-btn{{background:none;border:none;color:#8c959f;font-size:12px;cursor:pointer;padding:2px 4px}}
.text-btn:hover{{color:#24292f}}
/* ── content ── */
.content{{max-width:960px;margin:0 auto;padding:16px 24px}}
/* ── option card ── */
.option{{border:1px solid #d0d7de;border-radius:6px;margin-bottom:8px;overflow:hidden;border-left-width:3px}}
.option-investigate{{border-left-color:#e3b341}}
.option-safe{{border-left-color:#cf222e}}
.option-never{{border-left-color:#d0d7de}}
.option-never{{opacity:.7}}
.option-hidden{{display:none!important}}
/* ── summary row ── */
.option-summary{{list-style:none;padding:9px 14px;cursor:pointer;display:flex;align-items:center;gap:8px;user-select:none}}
.option-summary::-webkit-details-marker{{display:none}}
details[open]>.option-summary{{border-bottom:1px solid #d0d7de}}
.badge{{font-size:11px;font-weight:500;padding:1px 7px;border-radius:4px;white-space:nowrap;flex-shrink:0;border:1px solid transparent}}
.badge-investigate{{background:#fff8c5;color:#7d4e00;border-color:#e3b341}}
.badge-safe{{background:#ffebe9;color:#cf222e;border-color:#f7928a}}
.badge-never{{background:#f6f8fa;color:#8c959f;border-color:#d0d7de}}
.option-key{{font-family:"SFMono-Regular",Consolas,monospace;font-size:13px;font-weight:600;color:#0969da;word-break:break-all}}
.option-never .option-key{{color:#8c959f;font-weight:400}}
.found-tag{{font-size:11px;padding:1px 6px;border-radius:4px;margin-left:auto;flex-shrink:0;font-family:monospace;border:1px solid transparent}}
.found-none{{background:#ffebe9;color:#cf222e;border-color:#f7928a}}
.found-hit{{background:#dafbe1;color:#1a7f37;border-color:#82cfaa}}
/* ── card body ── */
.option-location{{font-size:11px;font-family:monospace;color:#8c959f;padding:6px 14px 2px}}
.django-tag{{font-size:11px;background:#fbefff;color:#8250df;padding:1px 6px;border-radius:4px;border:1px solid #d8b4fe;margin-left:6px}}
pre.decl{{background:#f6f8fa;color:#57606a;margin:0;padding:10px 14px;font-size:12px;line-height:1.5;overflow-x:auto;font-family:"SFMono-Regular",Consolas,monospace;border-top:1px solid #d0d7de}}
.usages-section{{padding:8px 14px 10px;border-top:1px solid #d0d7de}}
.usages-label{{font-size:11px;color:#8c959f;font-weight:500;margin-bottom:4px}}
.usage-file{{font-family:monospace;font-size:12px;color:#0969da;display:block;padding:1px 0;word-break:break-all}}
.no-usages{{padding:8px 14px;border-top:1px solid #ffd7d5;background:#ffebe9;font-size:12px;color:#cf222e;font-weight:500}}
.hint-tag{{font-size:11px;background:#fff8c5;color:#7d4e00;border:1px solid #e3b341;padding:1px 6px;border-radius:4px;margin-right:4px;white-space:nowrap}}
.db-tag{{font-size:11px;background:#fff7ed;color:#c2410c;border:1px solid #fed7aa;padding:1px 6px;border-radius:4px;margin-right:4px;white-space:nowrap;font-weight:500}}
.db-tag.disk{{background:#eff6ff;color:#1d4ed8;border-color:#bfdbfe}}
/* ── runtime-only cards ── */
.option-runtime{{border-left-color:#a78bfa;opacity:.8}}
.badge-runtime{{background:#ede9fe;color:#5b21b6;border:1px solid #c4b5fd}}
.stat-runtime{{background:#ede9fe;color:#5b21b6;border-color:#c4b5fd}}
/* ── git intro commit ── */
.git-intro{{border-top:1px solid #d0d7de}}
.git-intro-summary{{padding:6px 14px;font-size:12px;color:#8c959f;cursor:pointer;list-style:none;display:flex;align-items:center;gap:6px;flex-wrap:wrap}}
.git-intro-summary::-webkit-details-marker{{display:none}}
.git-intro-summary::before{{content:"▶";font-size:9px;transition:transform .15s}}
details.git-intro[open] .git-intro-summary::before{{transform:rotate(90deg)}}
.git-sha{{font-family:monospace;background:#f6f8fa;border:1px solid #d0d7de;padding:0 4px;border-radius:3px;font-size:11px;color:#0550ae}}
.git-msg{{color:#24292f;font-style:italic}}
pre.diff{{margin:0;padding:10px 14px;font-size:11px;line-height:1.45;overflow-x:auto;font-family:"SFMono-Regular",Consolas,monospace;background:#fff;border-top:1px solid #d0d7de}}
.d-add{{display:block;background:#e6ffec;color:#24292f}}
.d-del{{display:block;background:#ffebe9;color:#24292f}}
.d-hunk{{display:block;background:#ddf4ff;color:#0550ae}}
.d-meta{{display:block;color:#8c959f}}
.d-ctx{{display:block;color:#57606a}}
</style>
</head>
<body>
<div class="topbar">
  <span class="topbar-title">Options Inventory</span>
  <span class="stat stat-investigate" onclick="setStatus('INVESTIGATE')">{n_inv} investigate</span>
  <span class="stat stat-safe" onclick="setStatus('SAFE_CANDIDATE')">{n_safe} safe candidate</span>
  <span class="stat stat-never" onclick="setStatus('NEVER_DELETE')">{n_nev} never delete</span>
  {f'<span class="stat stat-runtime" onclick="setStatus(\'RUNTIME_ONLY\')">{n_runtime} runtime-only</span>' if n_runtime else ''}
  <span class="topbar-sha">sentry@{sentry_sha[:9]} · getsentry@{getsentry_sha[:9]}</span>
  <div class="controls">
    <button class="text-btn" onclick="expandAll()">expand all</button>
    <button class="text-btn" onclick="collapseAll()">collapse all</button>
    <button class="filter-btn active" data-status="all" onclick="setStatus('all')">all</button>
    <button class="filter-btn" data-status="INVESTIGATE" onclick="setStatus('INVESTIGATE')">investigate</button>
    <button class="filter-btn" data-status="SAFE_CANDIDATE" onclick="setStatus('SAFE_CANDIDATE')">safe candidate</button>
    <button class="filter-btn" data-status="NEVER_DELETE" onclick="setStatus('NEVER_DELETE')">never delete</button>
    {f'<button class="filter-btn" data-status="RUNTIME_ONLY" onclick="setStatus(\'RUNTIME_ONLY\')">runtime only</button>' if n_runtime else ''}
    <input class="search-box" type="search" placeholder="search key…" oninput="setSearch(this.value)">
    <span class="vis-count" id="vis-count">{total} options</span>
  </div>
</div>
<div class="content" id="options">
{''.join(cards)}
</div>
<script>
let currentStatus='all', searchQuery='';
function updateVisible(){{
  let n=0;
  document.querySelectorAll('.option').forEach(el=>{{
    const show=(currentStatus==='all'||el.dataset.status===currentStatus)
               &&(!searchQuery||el.dataset.key.includes(searchQuery));
    el.classList.toggle('option-hidden',!show);
    if(show)n++;
  }});
  document.getElementById('vis-count').textContent=n+' of {total} options';
}}
function setStatus(s){{
  currentStatus=s;
  document.querySelectorAll('.filter-btn').forEach(b=>b.classList.toggle('active',b.dataset.status===s));
  updateVisible();
}}
function setSearch(q){{searchQuery=q.toLowerCase();updateVisible();}}
function expandAll(){{document.querySelectorAll('.option:not(.option-hidden) details').forEach(d=>d.open=true);}}
function collapseAll(){{document.querySelectorAll('details').forEach(d=>d.open=false);}}
</script>
</body>
</html>''')


def _extract_relevant_hunk(diff: str, key: str) -> str:
    """Return the commit header + the specific hunk that introduced register("key")."""
    search = f'register("{key}"'
    lines = diff.split('\n')

    # Commit header ends at the first 'diff --git' or '@@' line.
    pre_end = next(
        (i for i, l in enumerate(lines) if l.startswith('diff --git') or l.startswith('@@')),
        min(8, len(lines)),
    )
    header = [l for l in lines[:pre_end] if l.strip()]

    # Slice remaining lines into hunks (each starting with '@@').
    rest = lines[pre_end:]
    hunk_starts = [i for i, l in enumerate(rest) if l.startswith('@@')]
    if not hunk_starts:
        return '\n'.join(header)

    for idx, start in enumerate(hunk_starts):
        end = hunk_starts[idx + 1] if idx + 1 < len(hunk_starts) else len(rest)
        hunk = rest[start:end]
        if any(search in l for l in hunk):
            return '\n'.join(header + [''] + hunk)

    return '\n'.join(header)


def _diff_to_html(snippet: str) -> str:
    """Render a diff snippet to HTML with per-line colour spans."""
    parts = []
    for line in snippet.split('\n'):
        escaped = _html.escape(line)
        if line.startswith('+') and not line.startswith('+++'):
            parts.append(f'<span class="d-add">{escaped}</span>')
        elif line.startswith('-') and not line.startswith('---'):
            parts.append(f'<span class="d-del">{escaped}</span>')
        elif line.startswith('@@'):
            parts.append(f'<span class="d-hunk">{escaped}</span>')
        elif line.startswith('commit ') or line.startswith('Author:') or line.startswith('Date:'):
            parts.append(f'<span class="d-meta">{escaped}</span>')
        else:
            parts.append(f'<span class="d-ctx">{escaped}</span>')
    return '\n'.join(parts)


_GIT_ENV = {**os.environ, 'GIT_TERMINAL_PROMPT': '0', 'GIT_PAGER': 'cat'}


def github_base_url(repo_root: str) -> str | None:
    """Return the GitHub HTTPS base URL for a repo (no trailing slash), or None."""
    try:
        r = subprocess.run(
            ['git', '-C', repo_root, 'remote', 'get-url', 'origin'],
            capture_output=True, text=True, timeout=5, env=_GIT_ENV,
        )
        url = r.stdout.strip()
        # git@github.com:org/repo.git  →  https://github.com/org/repo
        if url.startswith('git@github.com:'):
            url = 'https://github.com/' + url[len('git@github.com:'):]
        elif url.startswith('https://github.com/'):
            pass  # already correct
        else:
            return None
        return url.removesuffix('.git')
    except Exception:
        return None


def _git_log_first(
    repo_root: str,
    search: str,
    file_path: str | None,
    timeout: int = 45,
) -> str | None:
    """
    Run `git log --reverse -S search` and return the first matching line, or None.
    No --all: searching HEAD ancestry is fast and covers all merged options.
    All git calls here are read-only and safe to run in parallel.
    """
    cmd = [
        'git', '-C', repo_root, 'log', '--reverse',
        '--format=%H\t%an\t%ad\t%s', '--date=short', '--no-color',
        '-S', search,
    ]
    if file_path:
        cmd += ['--', file_path]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout, env=_GIT_ENV)
        return next((l for l in r.stdout.splitlines() if l), None)
    except Exception:
        return None


def fetch_first_commit(
    key: str, source_short: str, roots: list[str],
    github_urls: dict[str, str | None],
) -> dict | None:
    """
    Find the first commit that introduced register("key") in source_short.

    Tries search terms in order of precision:
      1. register("key"   — matches inline single-line registrations (sentry style)
      2. "key"            — matches multi-line registrations (getsentry style) and
                            any other context where the quoted key appears

    For each term, tries file-scoped first (fast), then whole-repo (catches moved files).
    """
    repo_root = next((r for r in roots if os.path.exists(os.path.join(r, source_short))), None)
    if not repo_root:
        return None

    first_line = None
    for search in [f'register("{key}"', f'"{key}"']:
        first_line = _git_log_first(repo_root, search, source_short)
        if first_line:
            break
        # File may have moved — retry without path restriction
        first_line = _git_log_first(repo_root, search, None)
        if first_line:
            break

    if not first_line:
        return None

    try:
        sha, author, date, message = first_line.split('\t', 3)
    except ValueError:
        return None

    diff = subprocess.run(
        ['git', '-C', repo_root, 'show', '--unified=4', '--no-color', sha, '--', source_short],
        capture_output=True, text=True, timeout=30, env=_GIT_ENV,
    )
    base = github_urls.get(repo_root)
    return {
        'sha': sha[:9], 'sha_full': sha,
        'author': author, 'date': date, 'message': message,
        'diff_snippet': _extract_relevant_hunk(diff.stdout, key),
        'github_url': f'{base}/commit/{sha}' if base else None,
    }


def fetch_investigate_commits(
    registrations: list[dict],
    safety_map: dict[str, str],
    roots: list[str],
) -> dict[str, dict | None]:
    """Parallel-fetch first-introduction commits for every INVESTIGATE option."""
    candidates = [r for r in registrations if safety_map[r['key']] in ('INVESTIGATE', 'SAFE_CANDIDATE')]
    if not candidates:
        return {}

    github_urls = {root: github_base_url(root) for root in roots}

    def _fetch(reg: dict) -> tuple[str, dict | None]:
        return reg['key'], fetch_first_commit(reg['key'], reg['source'], roots, github_urls)

    results: dict[str, dict | None] = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
        for key, commit in pool.map(_fetch, candidates):
            results[key] = commit
    return results


def get_sha(repo_path: str) -> str:
    try:
        r = subprocess.run(
            ['git', '-C', repo_path, 'rev-parse', 'HEAD'],
            capture_output=True, text=True, timeout=5,
        )
        return r.stdout.strip()
    except Exception:
        return 'unknown'


def shorten(path: str, roots: list[str]) -> str:
    for root in roots:
        if path.startswith(root + '/'):
            return path[len(root) + 1:]
    return path


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    parser = argparse.ArgumentParser(description='Extract option inventory with safety classification')
    parser.add_argument('--sentry', required=True)
    parser.add_argument('--getsentry', required=True)
    parser.add_argument(
        '--all', action='store_true',
        help='Output a human-readable full report instead of CSV',
    )
    parser.add_argument(
        '--html', action='store_true',
        help='Output a self-contained HTML report instead of CSV',
    )
    parser.add_argument(
        '--plan', metavar='FILE',
        help='Write a Markdown cleanup plan for coding agents to FILE',
    )
    parser.add_argument(
        '--color', action='store_true',
        help='Force color output (useful when piping to `less -R`)',
    )
    parser.add_argument(
        '--no-color', action='store_true', dest='no_color',
        help='Disable color output',
    )
    parser.add_argument(
        '--active-options', metavar='FILE',
        help='Path to active_options.json from enumerate_active_options.py. '
             'Defaults to <getsentry>/active_options.json if that file exists.',
    )
    args = parser.parse_args()

    # --- Load active options snapshot (auto-discovered or explicit) ---
    active_db_keys:      set[str] = set()
    active_disk_keys:    set[str] = set()
    active_all_keys:     set[str] = set()
    active_unknown_db:   set[str] = set()

    active_options_path = args.active_options or os.path.join(os.path.dirname(__file__), 'active_options.json')
    if os.path.exists(active_options_path):
        try:
            with open(active_options_path) as _af:
                _active = json.load(_af)
            active_db_keys    = set(_active.get('has_db_value', []))
            active_disk_keys  = set(_active.get('has_disk_value', []))
            active_all_keys   = set(_active.get('all_registered', []))
            active_unknown_db = set(_active.get('unknown_db_keys', []))
            print(
                f'Active options loaded from {active_options_path}: '
                f'{len(active_all_keys)} registered, '
                f'{len(active_db_keys)} db values, '
                f'{len(active_disk_keys)} disk values',
                file=sys.stderr,
            )
        except Exception as exc:
            print(f'WARNING: could not load active options from {active_options_path}: {exc}', file=sys.stderr)
    elif args.active_options:
        print(f'WARNING: --active-options file not found: {active_options_path}', file=sys.stderr)

    def option_category(key: str, in_static: bool, safety: str) -> str:
        """Assign a category for the master list."""
        if not in_static:
            if key.startswith('feature.'):
                return 'runtime/feature'
            if key.startswith('dynamic.'):
                return 'runtime/dynamic'
            return 'runtime/other'
        if safety == 'NEVER_DELETE':
            return 'static/active'
        return 'static/candidate'   # SAFE_CANDIDATE or INVESTIGATE

    roots = [args.sentry, args.getsentry]
    # Registration discovery stays scoped to the source subdirs to avoid
    # picking up test fixtures and generated files that call register().
    reg_search_dirs = [
        os.path.join(args.sentry, 'src/sentry'),
        os.path.join(args.getsentry, 'getsentry'),
    ]

    # --- Find registration files ---
    print('Finding registration files...', file=sys.stderr)
    registration_files: set[str] = set()
    for d in reg_search_dirs:
        try:
            r = subprocess.run(
                ['rg', '-l', '--type', 'py', r'options\.register\(|^register\(', d],
                capture_output=True, text=True, timeout=15,
            )
            for line in r.stdout.strip().split('\n'):
                if not line:
                    continue
                if '/tests/' in line or '/test_' in line or '/migrations/' in line:
                    continue
                if 'options/manager.py' in line:
                    continue
                registration_files.add(line)
        except (subprocess.TimeoutExpired, FileNotFoundError):
            pass

    # --- Extract registrations ---
    print('Extracting registrations...', file=sys.stderr)
    registrations: list[dict] = []
    for reg_file in sorted(registration_files):
        registrations.extend(extract_registrations(reg_file))
    registrations.sort(key=lambda r: r['key'])
    for reg in registrations:
        reg['source'] = shorten(reg['source'], roots)

    known_keys: set[str] = {r['key'] for r in registrations}
    all_keys: list[str] = [r['key'] for r in registrations]

    # --- options_mapper ---
    print('Parsing options_mapper...', file=sys.stderr)
    options_mapper = find_options_mapper(args.sentry)
    print(f'  {len(options_mapper)} mapped options found', file=sys.stderr)

    # --- AST pass ---
    print('AST pass: scanning Python files for options accessor calls...', file=sys.stderr)
    ast_result = ast_scan_repos(known_keys, roots, registration_files)
    print(
        f'  {sum(len(v) for v in ast_result.literal_usages.values())} literal call sites, '
        f'{len(ast_result.dynamic_access_files)} files with dynamic access',
        file=sys.stderr,
    )

    # --- String pass (all file types, full repos) ---
    print('String pass: searching all file types across full repos...', file=sys.stderr)
    string_usages = string_scan_repos(all_keys, roots, registration_files)

    # --- Merge: union of AST literal usages and string usages ---
    # found_by tracks which pass(es) found each key before merging:
    #   'ast'    — options.get/set/delete("key") call found by AST parse
    #   'string' — key string found somewhere (YAML, settings override, comment, etc.)
    #              but no explicit options.get() call site located
    #   'both'   — confirmed by both; highest confidence
    #   'none'   — no evidence of use in either repo
    def _found_by(key: str) -> str:
        in_ast = bool(ast_result.literal_usages.get(key))
        in_string = bool(string_usages.get(key))
        if in_ast and in_string:
            return 'both'
        if in_ast:
            return 'ast'
        if in_string:
            return 'string'
        return 'none'

    merged_usages: dict[str, set[str]] = {}
    for key in all_keys:
        merged_usages[key] = ast_result.literal_usages.get(key, set()) | string_usages.get(key, set())

    # --- Attribute dynamic-access files to bound their key reach ---
    print('Attributing dynamic-access files...', file=sys.stderr)
    file_attributions = attribute_dynamic_files(ast_result.dynamic_access_files)

    # B: admin-plane files (options.all/filter) are management interfaces, not application
    # use of a specific option.  Exclude them from the uncertainty calculation even if they
    # also happen to have incidental unresolved calls.
    truly_uncertain: dict[str, FileAttribution] = {
        path: attr for path, attr in file_attributions.items()
        if attr.unresolved and not attr.is_admin_plane
    }

    # A: Pre-read uncertain file contents for namespace plausibility checks.
    # Reading once here is much cheaper than per-key lookups inside classify().
    uncertain_contents: dict[str, str] = {}
    for path in truly_uncertain:
        try:
            with open(path, errors='replace') as f:
                uncertain_contents[path] = f.read()
        except OSError:
            uncertain_contents[path] = ''

    n_bounded = sum(1 for a in file_attributions.values() if a.bounded_prefixes or a.bounded_keys)
    n_admin   = sum(1 for a in file_attributions.values() if a.is_admin_plane)
    n_unresol = len(truly_uncertain)
    print(f'  {n_bounded} bounded  {n_admin} admin-plane  {n_unresol} app-unresolved', file=sys.stderr)

    def _namespace_prefix(key: str) -> str:
        """Return the two-segment prefix used for plausibility matching.
        e.g. 'api.organization_events.rate-limit-increased.orgs' → 'api.organization_events'
        Single-segment keys return themselves.
        """
        parts = key.split('.')
        return '.'.join(parts[:2]) if len(parts) >= 2 else key

    # --- Safety classification ---
    def classify(reg: dict) -> str:
        key = reg['key']
        flags = reg.get('flags', '')
        if merged_usages.get(key) or options_mapper.get(key) or 'FLAG_REQUIRED' in flags:
            return 'NEVER_DELETE'
        # Bounded dynamic patterns definitively cover this key.
        for attr in file_attributions.values():
            if (
                any(key.startswith(p) for p in attr.bounded_prefixes)
                or any(key.endswith(s) for s in attr.bounded_suffixes)
                or key in attr.bounded_keys
            ):
                return 'NEVER_DELETE'
        if not truly_uncertain:
            return 'SAFE_CANDIDATE'
        # A: Plausibility filter — only INVESTIGATE if a truly-uncertain (non-admin-plane)
        # file references this key's two-segment namespace prefix.  If billing/quota/rollout
        # code doesn't mention 'api.organization_events' anywhere, it can't be dynamically
        # constructing 'api.organization_events.rate-limit-increased.orgs'.
        prefix = _namespace_prefix(key)
        if any(prefix in content for content in uncertain_contents.values()):
            return 'INVESTIGATE'
        return 'SAFE_CANDIDATE'

    # --- Pre-compute maps so both output paths share them ---
    found_by_map = {key: _found_by(key) for key in all_keys}
    safety_map   = {reg['key']: classify(reg) for reg in registrations}

    # --- Hints: supplemental heuristic signals ---
    hints_map = {reg['key']: compute_hints(reg, safety_map[reg['key']], safety_map) for reg in registrations}

    # Palettes for each stream — each auto-detects its own TTY.
    p_out = _make_palette((sys.stdout.isatty() or args.color) and not args.no_color)
    p_err = _make_palette((sys.stderr.isatty() or args.color) and not args.no_color)

    # --- Output ---
    # Fetch first-introduction commits for INVESTIGATE options (HTML/plan only).
    first_commits: dict[str, dict | None] = {}
    if args.html or args.plan:
        print('Fetching first-introduction commits for INVESTIGATE options...', file=sys.stderr)
        first_commits = fetch_investigate_commits(registrations, safety_map, roots)
        found_count = sum(1 for v in first_commits.values() if v)
        print(f'  found {found_count}/{len(first_commits)} commits', file=sys.stderr)

    if args.plan:
        with open(args.plan, 'w') as plan_f:
            print_plan_report(
                registrations, merged_usages, options_mapper,
                found_by_map, safety_map, hints_map,
                file_attributions, first_commits, roots,
                get_sha(args.sentry), get_sha(args.getsentry),
                plan_f,
            )
        print(f'Plan written to {args.plan}', file=sys.stderr)

    if args.html:
        print_html_report(
            registrations, merged_usages, options_mapper,
            found_by_map, safety_map, hints_map, first_commits, roots,
            get_sha(args.sentry), get_sha(args.getsentry),
            active_db_keys=active_db_keys,
            active_disk_keys=active_disk_keys,
            active_all_keys=active_all_keys,
            active_unknown_db=active_unknown_db,
        )
    elif args.all:
        print_all_report(
            registrations, merged_usages, options_mapper,
            found_by_map, safety_map, hints_map, roots, p_out,
        )
    else:
        writer = csv.writer(sys.stdout)
        writer.writerow([
            'key', 'category', 'type', 'flags', 'default', 'source', 'lineno',
            'django_setting', 'found_by', 'safe_to_delete', 'hints',
            'has_db_value', 'has_disk_value', 'usages',
        ])
        # Static options (from register() scan)
        for reg in registrations:
            key = reg['key']
            usages_list = sorted(shorten(f, roots) for f in merged_usages.get(key, set()))
            writer.writerow([
                key,
                option_category(key, True, safety_map[key]),
                reg['type'],
                reg['flags'],
                reg['default'],
                reg['source'],
                reg['lineno'],
                options_mapper.get(key, ''),
                found_by_map[key],
                safety_map[key],
                ', '.join(hints_map[key]),
                'yes' if key in active_db_keys else '',
                'yes' if key in active_disk_keys else '',
                '; '.join(usages_list),
            ])
        # Runtime-only options: in active_options but not in the static scan
        if active_all_keys:
            for key in sorted(active_all_keys - known_keys):
                writer.writerow([
                    key,
                    option_category(key, False, ''),
                    '', '', '', '', '',   # type/flags/default/source/lineno
                    '',                  # django_setting
                    '',                  # found_by
                    'RUNTIME_ONLY',
                    '',                  # hints
                    'yes' if key in active_db_keys else '',
                    'yes' if key in active_disk_keys else '',
                    '',                  # usages
                ])
        # DB-only options: have a DB value but found by neither scan
        for key in sorted(active_unknown_db):
            writer.writerow([
                key,
                'db-only',
                '', '', '', '', '',
                '', '', 'DB_ONLY', '',
                'yes', '',
                '',
            ])

    # --- Summary to stderr ---
    e = p_err   # shorthand
    safety_counts: Counter = Counter(safety_map.values())
    source_counts: Counter = Counter(r['source'] for r in registrations)

    print(file=sys.stderr)
    print(f'sentry SHA:    {get_sha(args.sentry)}', file=sys.stderr)
    print(f'getsentry SHA: {get_sha(args.getsentry)}', file=sys.stderr)
    print(f'total options: {len(registrations)}', file=sys.stderr)
    print(file=sys.stderr)
    print(f'{e.bold}safety breakdown:{e.reset}', file=sys.stderr)
    for label in ('NEVER_DELETE', 'INVESTIGATE', 'SAFE_CANDIDATE'):
        sc = e.for_status(label)
        print(f'  {safety_counts[label]:4d}  {sc}{label}{e.reset}', file=sys.stderr)
    print(file=sys.stderr)
    print('registrations per file:', file=sys.stderr)
    for source, count in source_counts.most_common():
        print(f'  {count:4d}  {source}', file=sys.stderr)
    if ast_result.dynamic_access_files:
        print(file=sys.stderr)
        print(
            'files with dynamic options access (INVESTIGATE options may be consumed here):',
            file=sys.stderr,
        )
        for f in sorted(ast_result.dynamic_access_files):
            print(f'  {e.cyan}{shorten(f, roots)}{e.reset}', file=sys.stderr)

    # --- Candidates: the actionable list ---
    candidates = [r for r in registrations if safety_map[r['key']] in ('INVESTIGATE', 'SAFE_CANDIDATE')]
    if candidates:
        print(file=sys.stderr)
        n_safe = safety_counts.get('SAFE_CANDIDATE', 0)
        n_inv  = safety_counts.get('INVESTIGATE', 0)
        parts  = []
        if n_safe:
            parts.append(f'{e.for_status("SAFE_CANDIDATE")}{n_safe} SAFE_CANDIDATE{e.reset}')
        if n_inv:
            parts.append(f'{e.for_status("INVESTIGATE")}{n_inv} INVESTIGATE{e.reset}')
        print(
            f'{e.bold}--- {len(candidates)} candidate options ({", ".join(parts)}{e.bold}) ---{e.reset}',
            file=sys.stderr,
        )
        for reg in candidates:
            usages = sorted(shorten(f, roots) for f in merged_usages.get(reg['key'], set()))
            _print_entry(
                reg, safety_map[reg['key']], found_by_map[reg['key']],
                usages, options_mapper.get(reg['key'], ''),
                hints_map.get(reg['key'], []), e, sys.stderr,
            )


if __name__ == '__main__':
    main()
