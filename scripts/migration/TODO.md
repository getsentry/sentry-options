# TODO

## Known limitations of the methodology

- **Option keys computed at runtime.** If a key is read from a database,
  environment variable, or config file and never appears as a literal string
  in either repo, neither the AST pass nor the string pass will find it.
  No static analysis tool can detect this; requires manual audit.

- **Consumers outside both repos.** External scripts, ops tooling, or other
  services that read from the options database will not be found.

- **Dynamic `options.get(variable)` calls.** The AST pass flags every file
  containing such calls and surfaces them in the stderr summary. Any option
  with no static usages (`INVESTIGATE`) could in principle be the one being
  accessed. These files must be reviewed manually before treating any
  `INVESTIGATE` option as safe.

## Potential improvements

- **Database cross-reference.** The relevant tables are `sentry_option` (all
  non-control-silo contexts) and `sentry_controloption` (control silo only).
  Both have the same schema: `key`, `value`, `last_updated`, `last_updated_by`.
  `sentry_organizationoptions`, `sentry_projectoptions`, and
  `accounts_subscriptionoption` are unrelated to the global options registry.

  **Important:** there is no `last_read` column. Reads go through Redis cache
  and never touch the DB. `last_updated` reflects writes only. An option can
  be read millions of times per day and still show a `last_updated` from years
  ago if its value has never been changed from its initial set.

  The correct query is therefore not "not read in N months" but
  **"ever written to the DB at all"** — a DB row existing at all is evidence
  the option was deliberately configured at some point:

  ```sql
  -- Which SAFE_CANDIDATE keys have ever been written to the DB?
  -- Run on the production region DB (sentry_option) and control DB (sentry_controloption).
  SELECT key, last_updated, last_updated_by, value
  FROM sentry_option
  WHERE key IN (
      'processing.sourcemapcache-processor',
      'store.nodestore-stats-sample-rate',
      -- ... full SAFE_CANDIDATE list
  )
  UNION ALL
  SELECT key, last_updated, last_updated_by, value
  FROM sentry_controloption
  WHERE key IN ( /* same list */ );
  ```

  A secondary query for broader hygiene — all options last written more than
  6 months ago (useful for identifying candidates to propose for removal):

  ```sql
  SELECT key, last_updated, last_updated_by
  FROM sentry_option
  WHERE last_updated < NOW() - INTERVAL '6 months'
  ORDER BY last_updated ASC;
  ```

  Any key appearing in the first query should be treated as `NEVER_DELETE`
  regardless of static analysis. Results can be passed to the inventory script
  as `--db-used-keys keys.txt` (not yet implemented).

- **Git recency analysis.** Check when each INVESTIGATE option's registration
  line was last modified. Options whose registration hasn't been touched in 2+
  years with zero key mentions in commit messages are more likely dead.
  **Caveat:** likely to be noisy in a high-churn repo like sentry — reformats,
  mass-renames, and import-order changes will all update blame without meaning
  anything. Also requires careful `--ignore-rev` configuration to exclude
  mechanical commits. Treat as a weak supplemental signal only, not a
  definitive one.

## Code improvements

- **Default extraction broken for multi-element lists.** `re.search(r'default=([^,\)]+)', block)`
  stops at the first comma, so `default=["a", "b"]` captures `["a"`. Fix:
  track bracket depth when extracting the default value, similar to
  `_extract_block`.

- **Registration regex misses single-quoted keys and multiline whitespace.**
  `r'(?:options\.)?register\(\s*\n?\s*"([^"]+)"'` requires double quotes
  and handles only a single newline before the key. Rare in practice but
  worth hardening.
