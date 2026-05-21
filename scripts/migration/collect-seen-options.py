#!/usr/bin/env python3
import json
import math
import signal
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

PROJECT = "internal-sentry"
INTERVAL = 600  # seconds per window
LOOKBACK = 43200  # 12 hours

out_file = sys.argv[1] if len(sys.argv) > 1 else str(Path(__file__).parent / "seen.txt")

seen: set[str] = set()


def flush_and_exit(signum, frame):
    write_output()
    sys.exit(130)


def write_output():
    with open(out_file, "w") as f:
        f.write("\n".join(sorted(seen)))
        if seen:
            f.write("\n")
    print(f"\n=== {len(seen)} unique options written to {out_file} ===", file=sys.stderr)


signal.signal(signal.SIGINT, flush_and_exit)
signal.signal(signal.SIGTERM, flush_and_exit)

now = int(datetime.now(timezone.utc).timestamp())
start_of_range = now - LOOKBACK
total_windows = math.ceil(LOOKBACK / INTERVAL)

for i in range(total_windows):
    start = start_of_range + i * INTERVAL
    end = min(start + INTERVAL, now)
    start_ts = datetime.fromtimestamp(start, tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    end_ts = datetime.fromtimestamp(end, tz=timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")

    print(
        f"window {i + 1:3d}/{total_windows}  {start_ts} → {end_ts}",
        end="",
        flush=True,
        file=sys.stderr,
    )

    before = len(seen)

    result = subprocess.run(
        [
            "gcloud",
            "logging",
            "read",
            f'jsonPayload.event="option.seen" AND timestamp>="{start_ts}" AND timestamp<"{end_ts}"',
            f"--project={PROJECT}",
            "--format=json",
            "--limit=10000",
        ],
        capture_output=True,
        text=True,
    )

    if result.returncode != 0:
        print("  [warn] failed", file=sys.stderr)
        continue

    try:
        entries = json.loads(result.stdout)
    except json.JSONDecodeError:
        print("  [warn] invalid JSON", file=sys.stderr)
        continue

    for entry in entries:
        key = entry.get("jsonPayload", {}).get("option_key")
        if key:
            seen.add(key)

    new = len(seen) - before
    print(f"  (+{new} new, {len(seen)} total)", file=sys.stderr)

write_output()
