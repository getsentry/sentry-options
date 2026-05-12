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

- **Database non-default check.** Query the production `options_option` table
  for keys that have ever had a non-default value set. Any such key should be
  treated as `NEVER_DELETE` regardless of static analysis. Could be passed to
  the script as `--db-used-keys keys.txt`.

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
