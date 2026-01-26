"""An example usage of the Python options client library.

Every 3 seconds, prints out the value of example-option, float-option,
and bool-option.

Updating values in `../values` will be reflected in stdout.
Ctrl+C to exit.
"""
from __future__ import annotations

import time

from sentry_options import options


def get_string_value():
    return options('sentry-options-testing').get('example-option')


def get_float_value():
    return options('sentry-options-testing').get('float-option')


def get_bool_value():
    return options('sentry-options-testing').get('bool-option')


if __name__ == '__main__':
    from sentry_options import init
    init()

    while True:
        time.sleep(3)
        s, f, b = get_string_value(), get_float_value(), get_bool_value()
        print(f"values: {s} | {f} | {b}", flush=True)
