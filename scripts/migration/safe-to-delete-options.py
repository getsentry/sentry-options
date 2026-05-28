#!/usr/bin/env python3
"""
Produces a list of options that are safe to delete:
  - SAFE_CANDIDATE: no static usages found anywhere in sentry/getsentry,
    and the key's namespace prefix doesn't appear in any unresolved
    dynamic-access file (plausibility filter). seen.txt is intentionally
    ignored for SAFE_CANDIDATE — production "seen" events for these keys
    come from admin-plane sweeps (options.all()), not genuine reads.
"""
import csv
import sys
from pathlib import Path

HERE = Path(__file__).parent
INVENTORY_FILE = HERE / "inventory.csv"

safe = []
with INVENTORY_FILE.open() as f:
    for row in csv.DictReader(f):
        if row.get("safe_to_delete") == "SAFE_CANDIDATE":
            safe.append(row["key"])

print(f"# {len(safe)} SAFE_CANDIDATE options", file=sys.stderr)
for key in sorted(safe):
    print(key)
