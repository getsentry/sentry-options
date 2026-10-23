"""Log each change in a Getsentry option as the options client sees it.

Runs as the app container of an injected Getsentry-like Deployment. It never
talks to Kubernetes; the driver deploys values and times their arrival from
these logs.
"""
from __future__ import annotations

import json
import time

from sentry_options import init
from sentry_options import options
from sentry_options import UnknownNamespaceError
from sentry_options import UnknownOptionError


OPTIONS_NAMESPACE = 'getsentry'
OPTION = 'getsentry.options-dual-read-test'
LEGACY_VALUE = 5
POLL_SECONDS = 0.1


def get_option_new_first() -> int:
    """Model Getsentry's dual-read hook with new values checked before legacy.

    Getsentry checks the legacy store first today. The intended new-first path
    still uses isset() to distinguish an unset value from the schema default.
    This fixed option is registered as FLAG_AUTOMATOR_MODIFIABLE in Getsentry.
    """
    try:
        handle = options(OPTIONS_NAMESPACE)
        if handle.isset(OPTION):
            return handle.get(OPTION)
    except (UnknownNamespaceError, UnknownOptionError):
        pass
    return LEGACY_VALUE


def main() -> None:
    init()
    last = None
    while True:
        value = get_option_new_first()
        if value != last:
            print(json.dumps({'event': 'observed', 'value': value, 'wall': time.time()}), flush=True)
            last = value
        time.sleep(POLL_SECONDS)


if __name__ == '__main__':
    main()
