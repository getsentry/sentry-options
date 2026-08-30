#!/usr/bin/env python3
"""
Produces a list of options that are safe to delete:
  - SAFE_CANDIDATE: no static usages found anywhere in sentry/getsentry,
    and the key's namespace prefix doesn't appear in any unresolved
    dynamic-access file (plausibility filter).
  - Also NOT present in seen.txt (options observed live via genuine reads).
    A seen option is excluded even if static analysis found no usages —
    the relocation.daily-limit.large incident showed that the static pass
    can miss dynamic key construction via bare-imported accessor functions.
"""
import csv
import sys
from pathlib import Path

HERE = Path(__file__).parent
INVENTORY_FILE = HERE / "inventory.csv"
SEEN_FILE = HERE / "seen.txt"

seen: set[str] = set()
if SEEN_FILE.exists():
    for line in SEEN_FILE.read_text().splitlines():
        if line:
            seen.add(line.split("\t")[0])

safe = []
skipped_seen = []
with INVENTORY_FILE.open() as f:
    for row in csv.DictReader(f):
        if row.get("safe_to_delete") == "SAFE_CANDIDATE":
            key = row["key"]
            if key in seen:
                skipped_seen.append(key)
            else:
                safe.append(key)

print(f"# {len(safe)} SAFE_CANDIDATE options (unseen)", file=sys.stderr)
if skipped_seen:
    print(f"# {len(skipped_seen)} SAFE_CANDIDATE options excluded because they appear in seen.txt:", file=sys.stderr)
    for key in sorted(skipped_seen):
        print(f"#   {key}", file=sys.stderr)
for key in sorted(safe):
    print(key)
