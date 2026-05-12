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

The summary (SHAs, safety breakdown, dynamic-access files, candidate list)
always goes to stderr and appears in the terminal regardless of stdout redirect.

Pass `--color` to force ANSI colour when piping to `less -R`; `--no-color` to
suppress it.

## How usage detection works

Two passes run in sequence; results are **unioned** (conservative):

**AST pass (Python only):** Parses every Python file in both full repos with the
Python AST. Finds `options.get/set/delete("literal-key")` calls and records
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

## Safety classification

| Value | Meaning |
|-------|---------|
| `NEVER_DELETE` | Static usage found, in `options_mapper`, or `FLAG_REQUIRED` |
| `INVESTIGATE` | No static usage found, but dynamic `options.get(variable)` calls exist somewhere in the codebase |
| `SAFE_CANDIDATE` | No evidence of use found anywhere and no dynamic access patterns exist (unlikely in a large codebase) |

The `found_by` column records which pass(es) found usages: `ast`, `string`,
`both`, or `none`. `both` is highest confidence; `string`-only means the key
appeared somewhere (settings override, comment, YAML) but no explicit
`options.get()` call was located.

## Caveats and confidence limits

### What the script reliably proves
`NEVER_DELETE` options have genuine static evidence of use. The two-pass
approach is conservative: if either pass finds the key string anywhere in either
repo, the option is kept. These 691 options should not be deleted.

### What the script cannot prove
**Absence of use is not the same as safety to delete.** An `INVESTIGATE`
classification means "no static evidence found" — not "definitely unused."
Several gaps remain:

**1. The 41 dynamic-access files are not equivalent.**
They fall into distinct categories with different risk profiles:

- *Management plane* (`system_options.py`, `admin_options.py`,
  `configoptions.py`, `config.py`): these iterate `options.all()` or accept a
  key at runtime. They can touch *any* registered option, but this is the admin
  interface — not application use. An option that only surfaces here is a
  stronger deletion candidate than the `INVESTIGATE` label implies.

- *Bounded infrastructure patterns*: several files use f-strings or computed
  keys that only ever reach a specific namespace — e.g.
  `f"backpressure.high_watermarks.{name}"`, `f"outbox_replication.{model}.replication_version"`,
  `options.get(killswitch_name)` where `killswitch_name` comes from a static
  registry. These can only touch options within their namespace; INVESTIGATE
  options outside those namespaces are unaffected.

- *Unattributed files*: a handful of files have dynamic access whose key space
  has not yet been bounded. Until attributed, every INVESTIGATE option is
  technically uncertain with respect to these.

**2. Options set in the database are invisible.**
Operators can set any registered option via the admin UI or `sentry config set`.
A non-default value in the production database is strong evidence the option is
in use, even if no call site exists in source. This script has no database
access and cannot detect this.

**3. External consumers are invisible.**
Options read by Relay, ops tooling, or services outside both repos have no
Python call sites and will appear unused.

**4. Dynamic key construction beyond f-strings.**
Keys computed from database lookups, environment variables, or config files
never appear as literal strings. These are rare but undetectable by static
analysis.

**5. The registration pass may be incomplete.**
The regex `(?:options\.)?register\(` misses options registered via helper
functions or auto-registered at runtime (e.g. feature flags managed by
`flagpole`). Such options would be absent from the inventory entirely.

### Required steps before deleting any INVESTIGATE option

1. **Attribute the bounded dynamic-access files.** For each infrastructure
   pattern file, determine which key namespace it can reach. INVESTIGATE options
   outside all bounded namespaces are higher-confidence candidates.

2. **Check the production database.** Any option with a non-default value ever
   set should be treated as `NEVER_DELETE` regardless of static analysis.

3. **Verify with domain owners.** Options in namespaces owned by specific teams
   (e.g. `seer.*`, `relay.*`, `billing.*`) should be confirmed with those teams
   before deletion.

## Known limitations to fix (see TODO.md)

- Default extraction broken for multi-element list defaults
- Registration regex misses single-quoted keys and complex whitespace

## Output snapshot

`inventory.csv` in this directory is a snapshot from the SHAs shown in that
file's header row.
