# Finding unused options

Three independent signals are combined to identify options that are safe to delete.
An option must be absent from **all three** before it is considered a candidate.

## Signal 1 — Static analysis (`option_inventory.py`)

Scans the full sentry and getsentry source trees using two passes:

- **AST pass**: finds `options.get/set/delete/isset("literal-key")` calls in Python files.
- **String pass**: searches every file type (YAML, JS, shell, Helm charts, etc.) for the key string as a substring — intentionally over-counts to avoid false positives.

Options that survive both passes with no hits are classified `SAFE_CANDIDATE`. Options where a non-admin-plane file contains dynamic `options.get(variable)` calls that could plausibly reach the key's namespace are classified `INVESTIGATE`.

`option_inventory.py` also merges `active_options.json` (see Signal 2) to include runtime-registered options (Flagpole feature flags, saferollouts) that never appear in `register()` calls. These get `safe_to_delete = RUNTIME_ONLY` and are excluded from deletion consideration.

Output: `inventory.csv`

## Signal 2 — Runtime enumeration (`enumerate_active_options.py`)

Initialises a live getsentry instance and snapshots which options have non-default values in each storage layer (database, disk config). Options with a live database value are marked `has_db_value = yes` in `inventory.csv` and treated as `NEVER_DELETE` regardless of static analysis.

Output: `active_options.json`

## Signal 3 — Production observation (`collect-seen-options.py`)

Queries GCP structured logs for `jsonPayload.event = "option.seen"` — a log line emitted once per option per process lifetime the first time `options.get()` is called for that key. Walks a 24-hour window in 10-minute intervals to stay within response size limits.

Output: `seen.txt`

## Cross-referencing (`check-seen-options.py`, `safe-to-delete-options.py`)

`check-seen-options.py` verifies that no option in `seen.txt` is also a deletion candidate in `inventory.csv` — a mismatch means static analysis missed a real usage and the option must not be deleted.

`safe-to-delete-options.py` produces the final list: options classified `SAFE_CANDIDATE` by static analysis **and** absent from `seen.txt`.

## Workflow

```
# 1. Refresh the runtime snapshot (requires local Postgres)
cd ~/dev/getsentry
~/dev/sentry/.venv/bin/python ~/dev/sentry-options/scripts/migration/enumerate_active_options.py \
    > ~/dev/sentry-options/scripts/migration/active_options.json

# 2. Collect production observations (requires gcloud auth)
python ~/dev/sentry-options/scripts/migration/collect-seen-options.py

# 3. Regenerate the unified inventory
python ~/dev/sentry-options/scripts/migration/option_inventory.py \
    --sentry ~/dev/sentry \
    --getsentry ~/dev/getsentry \
    > ~/dev/sentry-options/scripts/migration/inventory.csv

# 4. Verify no seen options are wrongly flagged as candidates
python ~/dev/sentry-options/scripts/migration/check-seen-options.py

# 5. Get the safe-to-delete list
python ~/dev/sentry-options/scripts/migration/safe-to-delete-options.py
```

## Confidence and known gaps

Static analysis cannot see options consumed by external services (Relay, Seer, ops tooling) or referenced only in private infrastructure repos. The production observation signal partially covers this — if an option is being read anywhere in the fleet it will appear in `seen.txt` — but `seen.txt` reflects only the lookback window and only processes that ran during that period.

Before deleting any candidate, also confirm the option has no database value (`has_db_value` column in `inventory.csv`) and search any external service repos for the key string.
