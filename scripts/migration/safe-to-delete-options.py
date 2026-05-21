#!/usr/bin/env python3
"""
Produces a list of options that are absolutely safe to delete:
  - no detected usages in inventory.csv (static analysis found nothing)
  - not present in seen.txt (never observed in production via GCP logs)
"""
import csv
import sys
from pathlib import Path

HERE = Path(__file__).parent
SEEN_FILE = HERE / "seen.txt"
INVENTORY_FILE = HERE / "inventory.csv"

seen = set(SEEN_FILE.read_text().splitlines())

safe = []
with INVENTORY_FILE.open() as f:
    for row in csv.DictReader(f):
        key = row["key"]
        if row.get("safe_to_delete") == "SAFE_CANDIDATE" and key not in seen:
            safe.append(key)

print(f"# {len(safe)} options with no static usages and never seen in production", file=sys.stderr)
for key in sorted(safe):
    print(key)
