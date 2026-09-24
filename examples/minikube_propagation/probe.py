"""Measure server-side apply PATCH start to the first read of each new option value."""

from __future__ import annotations

import json
import os
import ssl
import time
from datetime import datetime, timezone
from pathlib import Path
from urllib.request import Request, urlopen

from sentry_options import init, options


OPTIONS_NAMESPACE = "getsentry"
OPTION = "getsentry.options-dual-read-test"
CONFIGMAP = "sentry-options-getsentry"
INITIAL_VALUE = 100
UPDATED_VALUE = 101
SERVICE_ACCOUNT = Path("/var/run/secrets/kubernetes.io/serviceaccount")
POLL_SECONDS = 0.1


def report(event: str, **fields: object) -> None:
    print(json.dumps({"event": event, **fields}), flush=True)


def patch_configmap(
    url: str, namespace: str, token: str, context: ssl.SSLContext, value: int
) -> tuple[float, float]:
    generated_at = datetime.now(timezone.utc).isoformat(timespec="microseconds")
    values = {"options": {OPTION: value}, "generated_at": generated_at}
    patch = {
        "apiVersion": "v1",
        "kind": "ConfigMap",
        "metadata": {
            "name": CONFIGMAP,
            "namespace": namespace,
            "annotations": {"generated_at": generated_at},
        },
        "data": {"values.json": json.dumps(values, separators=(",", ":"))},
    }
    request = Request(
        url,
        data=json.dumps(patch).encode(),
        headers={
            "Authorization": f"Bearer {token}",
            "Content-Type": "application/apply-patch+yaml",
        },
        method="PATCH",
    )

    started = time.monotonic()
    with urlopen(request, context=context, timeout=30) as response:
        response.read()
    return started, time.monotonic()


def main() -> None:
    samples = int(os.environ["SAMPLE_COUNT"])
    sample_timeout = float(os.environ["SAMPLE_TIMEOUT_SECONDS"])
    namespace = SERVICE_ACCOUNT.joinpath("namespace").read_text().strip()
    token = SERVICE_ACCOUNT.joinpath("token").read_text().strip()
    context = ssl.create_default_context(cafile=str(SERVICE_ACCOUNT / "ca.crt"))
    host = os.environ["KUBERNETES_SERVICE_HOST"]
    port = os.environ["KUBERNETES_SERVICE_PORT_HTTPS"]
    url = (
        f"https://{host}:{port}/api/v1/namespaces/{namespace}/configmaps/{CONFIGMAP}"
        "?fieldManager=sentry-options-propagation&force=true"
    )

    init()
    handle = options(OPTIONS_NAMESPACE)
    if handle.get(OPTION) != INITIAL_VALUE:
        raise RuntimeError(f"Expected {OPTION} to start as {INITIAL_VALUE}")
    report("ready", samples=samples)

    for iteration in range(1, samples + 1):
        value = UPDATED_VALUE if iteration % 2 else INITIAL_VALUE
        started, patch_finished = patch_configmap(url, namespace, token, context, value)
        deadline = started + sample_timeout
        while True:
            observed_value = handle.get(OPTION)
            observed_at = time.monotonic()
            if observed_value == value:
                report(
                    "sample",
                    iteration=iteration,
                    latency_seconds=observed_at - started,
                    api_patch_seconds=patch_finished - started,
                )
                break
            if observed_at >= deadline:
                raise TimeoutError(f"Sample {iteration} did not propagate within {sample_timeout}s")
            time.sleep(POLL_SECONDS)

    report("done", samples=samples)


if __name__ == "__main__":
    main()
