"""
Reproduce SIGSEGV (Linux) / deadlock (macOS) in forked child processes
caused by sentry-options 1.0.7+.

Run from the clients/python venv:
    clients/python/.venv/bin/python repro_fork_segfault.py
"""
from __future__ import annotations

import faulthandler
import json
import multiprocessing
import os
import signal
import time

from sentry_options import init
from sentry_options import options

faulthandler.enable()
faulthandler.register(signal.SIGUSR1)

OPTIONS_DIR = os.path.join(os.path.dirname(__file__), 'sentry-options')
os.environ['SENTRY_OPTIONS_DIR'] = OPTIONS_DIR


VALUES_FILE = os.path.join(
    OPTIONS_DIR, 'values', 'sentry-options-testing', 'values.json',
)


def worker_task(i: int) -> str:
    """Runs in a forked child. options.get() triggers ensure_alive -> respawn."""
    signal.alarm(10)
    try:
        opts = options('sentry-options-testing')
        val = opts.get('string-option')
        signal.alarm(0)
        return f"worker {i} (pid={os.getpid()}): got '{val}'"
    except Exception as e:
        signal.alarm(0)
        return f"worker {i} (pid={os.getpid()}): error: {e}"


def mutate_values_file():
    """Actually modify the values file content to guarantee mtime change."""
    with open(VALUES_FILE) as f:
        data = json.load(f)

    # Toggle a value to force a real content + mtime change
    current = data['options'].get('string-option', '')
    data['options']['string-option'] = 'mutated' if current != 'mutated' else 'toggled'

    with open(VALUES_FILE, 'w') as f:
        json.dump(data, f, indent=2)


def main():
    print(f"Parent pid={os.getpid()}")

    init()

    opts = options('sentry-options-testing')
    print(f"Parent read: string-option = {opts.get('string-option')!r}")

    # Mutate the values file to force a watcher reload in the parent.
    # This triggers: reload_values -> emit_reload_spans -> get_sentry_hub()
    # which initializes the Sentry Hub with DefaultTransportFactory (HTTP threads).
    print('Mutating values file to trigger watcher reload...')
    mutate_values_file()

    # Wait for watcher to detect change (polls every 5s)
    print('Waiting 8s for watcher to detect change...')
    time.sleep(8)

    # Verify the parent picked up the change
    print(f"Parent read after reload: string-option = {opts.get('string-option')!r}")

    # Now fork. Children inherit the Sentry Hub with its dead transport thread.
    num_workers = 4
    num_tasks = 8
    print(f"\nForking {num_workers} workers, running {num_tasks} tasks...")
    print('Workers have a 10s alarm timeout.\n')

    ctx = multiprocessing.get_context('fork')
    try:
        with ctx.Pool(processes=num_workers) as pool:
            async_result = pool.map_async(worker_task, range(num_tasks))
            try:
                results = async_result.get(timeout=30)
                print(f"\nAll {len(results)} tasks returned successfully:")
                for r in results:
                    print(f"  {r}")
            except multiprocessing.TimeoutError:
                print('\nTIMEOUT: Workers deadlocked')
                pool.terminate()
    except Exception as e:
        print(f"\nPool error (worker likely crashed): {e}")

    # Restore the values file
    mutate_values_file()


if __name__ == '__main__':
    main()
