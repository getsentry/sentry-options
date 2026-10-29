#!/usr/bin/env python3
"""
Cross-references seen.txt (options observed live in GCP) against inventory.csv.
Any option seen in production but with no detected static usages is a false-positive
deletion candidate and must not be deleted.
"""
import csv
import sys
from pathlib import Path

HERE = Path(__file__).parent
SEEN_FILE = HERE / "seen.txt"
INVENTORY_FILE = HERE / "inventory.csv"

seen = {line.split("\t")[0] for line in SEEN_FILE.read_text().splitlines() if line}

inventory: dict[str, dict] = {}
with INVENTORY_FILE.open() as f:
    for row in csv.DictReader(f):
        inventory[row["key"]] = row

candidates = {k for k, r in inventory.items() if r.get("safe_to_delete") in ("SAFE_CANDIDATE", "INVESTIGATE")}
not_in_inventory = seen - inventory.keys()
false_positives = seen & candidates

print(f"seen:             {len(seen):4d} options")
print(f"inventory:        {len(inventory):4d} options")
print(f"deletion candidates: {len(candidates):4d} options")
print()

if false_positives:
    print(f"PROBLEM: {len(false_positives)} option(s) seen in production but flagged as deletion candidates:")
    for key in sorted(false_positives):
        status = inventory[key].get("safe_to_delete", "?")
        print(f"  [{status}] {key}")
else:
    print("ok: no seen options overlap with deletion candidates")

if not_in_inventory:
    print()
    print(f"note: {len(not_in_inventory)} seen option(s) not in inventory (feature.* / runtime-registered):")
    for key in sorted(not_in_inventory):
        print(f"  {key}")

sys.exit(1 if false_positives else 0)
