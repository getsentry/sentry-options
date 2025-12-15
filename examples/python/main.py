"""An example usage of the Python options client library.

Every 3 seconds, prints out the value of example-option, float-option,
and bool-option.

Updating values in `../values` will be reflected in stdout.
Ctrl+C to exit.
"""
from __future__ import annotations

import time

from sentry_options import init
from sentry_options import options

init()
testing = options('testing')

while True:
    time.sleep(3)
    string_value = testing.get('example-option')
    float_value = testing.get('float-option')
    bool_value = testing.get('bool-option')
    print(f"values: {string_value} | {float_value} | {bool_value}", flush=True)
