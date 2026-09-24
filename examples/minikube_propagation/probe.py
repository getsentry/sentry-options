"""Measure server-side apply PATCH start to a new-first dual-read."""
from __future__ import annotations

import json
import os
import ssl
import time
from datetime import datetime
from datetime import timezone
from pathlib import Path
from urllib.request import Request
from urllib.request import urlopen

from sentry_options import init
from sentry_options import options
from sentry_options import UnknownNamespaceError
from sentry_options import UnknownOptionError


OPTIONS_NAMESPACE = 'getsentry'
OPTION = 'getsentry.options-dual-read-test'
CONFIGMAP = 'sentry-options-getsentry'
INITIAL_VALUE = 100
UPDATED_VALUE = 101
LEGACY_VALUE = 5
SERVICE_ACCOUNT = Path('/var/run/secrets/kubernetes.io/serviceaccount')
POLL_SECONDS = 0.1


def report(event: str, **fields: object) -> None:
    print(json.dumps({'event': event, **fields}), flush=True)


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


def patch_configmap(
    url: str, namespace: str, token: str, context: ssl.SSLContext, value: int,
) -> tuple[float, float]:
    generated_at = datetime.now(timezone.utc).isoformat(timespec='microseconds')
    values = {'options': {OPTION: value}, 'generated_at': generated_at}
    patch = {
        'apiVersion': 'v1',
        'kind': 'ConfigMap',
        'metadata': {
            'name': CONFIGMAP,
            'namespace': namespace,
            'annotations': {'generated_at': generated_at},
        },
        'data': {'values.json': json.dumps(values, separators=(',', ':'))},
    }
    request = Request(
        url,
        data=json.dumps(patch).encode(),
        headers={
            'Authorization': f'Bearer {token}',
            'Content-Type': 'application/apply-patch+yaml',
        },
        method='PATCH',
    )

    started = time.monotonic()
    with urlopen(request, context=context, timeout=30) as response:
        response.read()
    return started, time.monotonic()


def main() -> None:
    samples = int(os.environ['SAMPLE_COUNT'])
    sample_timeout = float(os.environ['SAMPLE_TIMEOUT_SECONDS'])
    namespace = SERVICE_ACCOUNT.joinpath('namespace').read_text().strip()
    token = SERVICE_ACCOUNT.joinpath('token').read_text().strip()
    context = ssl.create_default_context(cafile=str(SERVICE_ACCOUNT / 'ca.crt'))
    host = os.environ['KUBERNETES_SERVICE_HOST']
    port = os.environ['KUBERNETES_SERVICE_PORT_HTTPS']
    url = (
        f'https://{host}:{port}/api/v1/namespaces/{namespace}/configmaps/{CONFIGMAP}'
        '?fieldManager=sentry-options-propagation&force=true'
    )

    init()
    if get_option_new_first() != INITIAL_VALUE:
        raise RuntimeError(f'Expected new-first dual-read of {OPTION} to start as {INITIAL_VALUE}')
    report('ready', samples=samples)

    for iteration in range(1, samples + 1):
        value = UPDATED_VALUE if iteration % 2 else INITIAL_VALUE
        started, patch_finished = patch_configmap(url, namespace, token, context, value)
        deadline = started + sample_timeout
        while True:
            observed_value = get_option_new_first()
            observed_at = time.monotonic()
            if observed_value == value:
                report(
                    'sample',
                    iteration=iteration,
                    latency_seconds=observed_at - started,
                    api_patch_seconds=patch_finished - started,
                )
                break
            if observed_at >= deadline:
                raise TimeoutError(f'Sample {iteration} did not propagate within {sample_timeout}s')
            time.sleep(POLL_SECONDS)

    report('done', samples=samples)


if __name__ == '__main__':
    main()
