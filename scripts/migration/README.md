# Options inventory

```bash
python scripts/migration/option_inventory.py \
    --sentry ~/dev/sentry \
    --getsentry ~/dev/getsentry \
    > inventory.csv
```

Output modes:

| Flag | Output |
|------|--------|
| *(default)* | CSV to stdout |
| `--all` | Colour-coded terminal report |
| `--html` | Self-contained HTML report (recommended) |
| `--plan FILE` | Markdown cleanup plan for a coding agent |

The summary (SHAs, safety breakdown, dynamic-access files, candidate list)
always goes to stderr and appears in the terminal regardless of stdout redirect.

Pass `--color` to force ANSI colour when piping to `less -R`; `--no-color` to
suppress it.

## How usage detection works

Two passes run in sequence; results are **unioned** (conservative):

**AST pass (Python only):** Parses every Python file in both full repos with the
Python AST. Finds `options.get/set/delete/isset("literal-key")` calls and records
the exact key and file. Also detects `options.get(variable)` — where the key
is not a string literal — and flags those files as containing *dynamic access*.

**String pass (all file types, full repos):** Searches every text file
(Python, YAML, JS, shell, Helm charts, etc.) for literal key strings.
Over-counts intentionally: if the key string appears anywhere outside a
registration file, the option is flagged as used.

**options_mapper cross-reference:** Options promoted to Django settings via
`options_mapper` in sentry's `initializer.py` are automatically classified
`NEVER_DELETE` regardless of whether `options.get()` call sites are found,
because application code reads them as `settings.EMAIL_HOST` etc. Both the
initial dict *and* the conditional `.update()` block are captured via AST.

**Dynamic attribution:** Files with unresolved dynamic access are analysed to
bound the key namespaces they can reach (f-string prefixes, static registries,
class attribute enumeration). Management-plane files (`options.all/filter`) are
separated from application-code files. Only files that are both non-admin-plane
*and* unresolved contribute to uncertainty for a given option.

**Plausibility filter:** For an option to remain `INVESTIGATE` rather than
`SAFE_CANDIDATE`, at least one truly-uncertain file must contain the option's
two-segment namespace prefix (e.g. `api.organization_events`) as a substring.
If no uncertain file references the namespace, it cannot be dynamically
constructing that key.

## Safety classification

| Value | Meaning |
|-------|---------|
| `NEVER_DELETE` | Static usage found, covered by a bounded dynamic pattern, in `options_mapper`, or `FLAG_REQUIRED` |
| `INVESTIGATE` | No static usage found; a non-admin-plane file with unresolved dynamic access references this option's namespace prefix |
| `SAFE_CANDIDATE` | No static usage anywhere, and no plausible dynamic access path |

The `found_by` column records which pass(es) found usages: `ast`, `string`,
`both`, or `none`. `both` is highest confidence; `string`-only means the key
appeared somewhere (settings override, comment, YAML) but no explicit
`options.get()` call was located.

---

## All the ways an option can be referenced

This section enumerates every known referencing pattern and states whether
the script detects it. It is the basis for the confidence analysis below.

### Detected by both AST pass and string pass

These patterns are covered by both independent methods; missing one requires
both to fail simultaneously.

| Pattern | Notes |
|---------|-------|
| `options.get("key")` | Primary read path |
| `options.set("key", value)` | Runtime override |
| `options.delete("key")` | Runtime clear |
| `options.isset("key")` | Existence check |

### Detected by string pass only

The string `"key"` appears as a literal in the file even though the AST pass
does not recognise the calling convention.

| Pattern | Notes |
|---------|-------|
| `options.lookup_key("key")` | Key validation |
| `options.can_update("key", ...)` | Admin write-path check |
| `options.default_value("key")` | Default introspection |
| `from sentry import options as opts; opts.get("key")` | Aliased import |
| `SENTRY_OPTIONS["key"]` | Direct dict access in settings files |
| `SENTRY_DEFAULT_OPTIONS["key"]` | Default dict access |
| `settings.SENTRY_OPTIONS["key"]` | Settings-level dict access |
| `settings.SENTRY_OPTIONS.get("key")` | Same, with `.get()` |
| `override_options({"key": value})` | Test context manager |
| `@override_options({"key": value})` | Test decorator |
| `with self.options({"key": value}):` | Legacy test helper |
| Key string in YAML, JSON, shell, Markdown, Helm templates | Config files, docs |
| Key string in test fixtures or snapshot files | Test data |

### Detected by dynamic attribution

These patterns do not contain the full literal key string but are resolved by
analysing the dynamic-access files.

| Pattern | Notes |
|---------|-------|
| `options.get(f"backpressure.high_watermarks.{name}")` | f-string with **literal prefix** — the fixed namespace comes first |
| `options.get(f"{self.interface_id}.disallow-new-enrollment")` | f-string with **literal suffix** — the fixed name comes last |
| `key = f"prefix.{var}"; options.get(key)` | Locally assigned f-string (prefix or suffix variant) |
| `options.get(killswitch_name)` where name ∈ `ALL_KILLSWITCH_OPTIONS` | Static registry enumeration |
| `options.get(self.enabled_option_name)` where attr is a class-level string constant | Class attribute enumeration |

### Detected by options_mapper cross-reference

| Pattern | Notes |
|---------|-------|
| `settings.EMAIL_HOST` (promoted from `mail.host`) | Any key in `options_mapper` or its `.update()` blocks is `NEVER_DELETE` even with no `options.get()` call sites |

### Not detected — known gaps

These patterns can reference an option without triggering either pass or any
attribution logic.

| Pattern | Gap | Scope |
|---------|-----|-------|
| Option key hardcoded in an **external repo** (Relay, Seer, ops tooling) | Repos not searched | `relay.*`, `seer.*`, any option a non-sentry service reads |
| **Production database** has a non-default value set by an operator or CI | No DB access | Any registered option |
| Dynamic construction from **non-adjacent parts**: `ns = "api"; key = f"{ns}.org_events.rate"` | Neither segment matches full key | Theoretical; no known real instances |
| **Infrastructure configs in private repos** (Terraform, Puppet, Helm in deploy repos) | Repos not searched | System/Redis/storage-level options |
| Options **only written, never read**: `SENTRY_OPTIONS["key"] = value` in getsentry settings with no corresponding `options.get("key")` anywhere | Classified as NEVER_DELETE (conservative), but these options may be set without ever being consumed | Low practical impact |
| **Suffix f-strings where the literal suffix alone is not unique enough**: `options.get(f"{var}.some-suffix")` where the suffix appears in comments but the full key (`foo.some-suffix`) is never mentioned as a complete string anywhere | The string pass finds the comment; the dynamic attribution handles the case when the suffix is distinctive (e.g. `.disallow-new-enrollment`) — **but if the full key never appears as a literal anywhere, and the suffix appears only in a doc comment with a different prefix, the plausibility filter may still miss it.** | Verified real incident: `recovery.disallow-new-enrollment` was incorrectly deleted because `base.py` documents the pattern with `sms.disallow-new-enrollment` as an example but never mentions `recovery.disallow-new-enrollment` by name. Restored in the same commit. |

---

## Confidence analysis for SAFE_CANDIDATE

After the first round of deletions (94 options removed from sentry), the current
run produces approximately **10 SAFE_CANDIDATES** (all in getsentry) and **5 INVESTIGATE**.

### Why the in-repo coverage is very high

The string pass is a strong backstop: it catches every occurrence of the key
string as a substring in any text file in either repo, regardless of context
(comments, YAML, test fixtures, aliased calls, dict access). Any in-repo usage
that involves the literal key string — which is virtually all of them — is
detected. The AST pass independently confirms semantic call sites.

The result: within sentry and getsentry, we have near-complete coverage of all
known referencing patterns. The probability of a false positive from within the
two repos is very low.

### The realistic false-positive scenarios

**1. External service consumers (~4 specific options)**

`relay.ourlogs-ingestion.sample-rate` is the clearest risk: the Relay Rust
codebase likely has this key as a string literal, but we do not search that
repo. The same concern applies to any option in a namespace that maps to a
separate service. Before deleting any `relay.*` or `seer.*` option, search the
corresponding service repo.

**2. Production database state (all 104 options)**

Any option with a non-default value ever written to `options_option` by an
operator, a migration, or an automated script is in use regardless of what
source code shows. This is the highest-confidence check available and cannot be
replicated by static analysis. It is a required step in the cleanup plan for
every candidate.

**3. Private infrastructure repos (~3–5 specific options)**

`system.root-api-key`, `redis.options`, `configurations.storage.backend`,
`configurations.storage.options` have names that suggest infrastructure-level
configuration. Ops tooling, Puppet manifests, or Terraform in private repos may
reference these keys. Treat these with extra caution and confirm with the
infrastructure team.

### Breakdown by confidence tier

| Tier | Count (approx.) | Criteria |
|------|-----------------|---------|
| **Highest** | ~85 | Application-level feature flags and rollout controls in clearly internal namespaces (`processing.*`, `store.*`, `grouping.*`, `apitoken.*`, `performance.*`, `issues.*`, `profiling.*`, etc.) with `deprecation_note` or `all_peers_used` hints, and no plausible external consumer |
| **High** | ~15 | Options without hints, in internal namespaces, with a plausible "dead rollout flag" explanation from their git introduction commit |
| **Needs extra care** | ~4 | `relay.ourlogs-ingestion.sample-rate`, `system.root-api-key`, `redis.options`, `configurations.storage.*` — external consumer or infra-config risk |

### Required steps before deleting any SAFE_CANDIDATE

1. **Re-run the static search at deletion time.** The inventory may be stale.
   `rg -l '<key>' ~/dev/sentry ~/dev/getsentry` takes seconds and catches
   anything merged since the inventory was generated.

2. **Check the production database.** Non-default value in `options_option` →
   `NEVER_DELETE`. This is mandatory for every candidate, not optional.

3. **Search external repos** for options in `relay.*`, `seer.*`, or other
   service-namespaced keys.

4. **Confirm with infra team** for `system.*`, `redis.*`,
   `configurations.storage.*` before deleting.

---

## Runtime option enumeration

`enumerate_active_options.py` initialises a live getsentry instance and
snapshots which options have non-default values from each storage location.
It is complementary to the static inventory — the static scan cannot see
database state, and this script cannot see code call sites.

```bash
cd ~/dev/getsentry
~/dev/sentry/.venv/bin/python3 \
    ~/dev/sentry-options/scripts/migration/enumerate_active_options.py \
    [--db-name <name>] \
    > ~/dev/sentry-options/scripts/migration/active_options.json
```

Use `--db-name sentry` if your local database is named `sentry` rather than
`getsentry` (common in single-repo dev setups). The script uses
`skip_service_validation=True` so Redis/Kafka do not need to be running;
only Postgres is required for the DB query.

Output fields:

| Field | Meaning |
|-------|---------|
| `all_registered` | Every option registered at runtime (includes `feature.*` and `dynamic.*` options auto-registered by flagpole — **307 more than the static inventory finds**) |
| `has_db_value` | Options with an explicit value in the database (`sentry_option` / `sentry_controloption` table) — the primary signal for live options |
| `has_disk_value` | Options set in `settings.SENTRY_OPTIONS` by getsentry's Python settings files |
| `has_default_override` | Options with entries in `settings.SENTRY_DEFAULT_OPTIONS` |
| `has_any_value` | Union of all three |
| `unknown_db_keys` | Keys in the DB but absent from the static inventory (registered at runtime or via a path the regex missed) |

### What the snapshot reveals

**The 307 runtime-only options** are `feature.*` and `dynamic.saferollouts.*`
entries auto-registered by flagpole at startup. These never appear in the
static scan because they are not registered via `register()` calls in source
files. They are outside the scope of the current migration effort but should
not be deleted from the registration machinery.

**The 93 disk-configured options** all appear as `NEVER_DELETE` in the static
inventory — confirming the string pass correctly caught every `SENTRY_OPTIONS`
assignment in the getsentry settings files.

**The `has_db_value` list** is the most important output for cross-referencing
against `SAFE_CANDIDATE` options. Any SAFE_CANDIDATE appearing here has a
live database value and should not be deleted without first clearing that value
or understanding why it was set.

### Cross-referencing with the static inventory

```python
import csv, json

with open('active_options.json') as f:
    active = json.load(f)

with open('inventory.csv') as f:
    inventory = {r['key']: r for r in csv.DictReader(f)}

db_set = set(active['has_db_value'])
candidates = {k: r for k, r in inventory.items()
              if r['safe_to_delete'] in ('SAFE_CANDIDATE', 'INVESTIGATE')}

risky = {k: r for k, r in candidates.items() if k in db_set}
if risky:
    print(f'{len(risky)} SAFE_CANDIDATEs have DB values — do not delete:')
    for k in risky:
        print(f'  {k}')
else:
    print('No SAFE_CANDIDATEs have DB values in this environment.')
```

`active_options.json` in this directory is a snapshot from a local dev
instance. It reflects the state of that environment only — production may
have additional DB values.

## Known limitations to fix (see TODO.md)

- Default extraction broken for multi-element list defaults
- Registration regex misses single-quoted keys and complex whitespace

## Output snapshot

`inventory.csv` in this directory is a snapshot from the SHAs shown in that
file's header row.
