"""Report in-pod ConfigMap apply PATCH to client get latency.

Run with `make test-minikube-propagation-latency`. This is an opt-in integration
probe, not a CI latency gate: kubelet sync timing varies.
"""
from __future__ import annotations

import argparse
import json
import math
import statistics
import subprocess
import sys
import time
import uuid
from typing import Any


CONTEXT = 'minikube'
CONFIGMAP = 'sentry-options-getsentry'
FEATURES_CONFIGMAP = 'sentry-options-getsentry-features'
POD = 'sentry-options-latency'
OPTIONS_NAMESPACE = 'getsentry'
OPTION = 'getsentry.options-dual-read-test'
IMAGE = 'sentry-options-propagation:local'
DEFAULT_SAMPLE_COUNT = 5


def kubectl(*args: str, input_text: str | None = None) -> str:
    result = subprocess.run(
        ['kubectl', '--context', CONTEXT, *args],
        input=input_text,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise RuntimeError(f"kubectl {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout


def apply(resource: dict[str, Any]) -> None:
    kubectl('apply', '-f', '-', input_text=json.dumps(resource))


def events(namespace: str) -> list[dict[str, Any]]:
    logs = kubectl('-n', namespace, 'logs', POD)
    found = []
    for line in logs.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(event, dict) and isinstance(event.get('event'), str):
            found.append(event)
    return found


def wait_for_event(namespace: str, name: str, timeout: float) -> list[dict[str, Any]]:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            found = events(namespace)
        except RuntimeError:
            found = []
        if any(event['event'] == name for event in found):
            return found
        time.sleep(0.5)
    status = kubectl('-n', namespace, 'get', 'pod', POD, '-o', 'json')
    logs = kubectl('-n', namespace, 'logs', POD)
    raise TimeoutError(f'Timed out waiting for {name}. Pod status: {status}. Logs: {logs}')


def run(image: str, sample_count: int, sample_timeout: float, request_pod_refresh: bool) -> None:
    namespace = f"sentry-options-latency-{uuid.uuid4().hex[:8]}"
    kubectl("create", "namespace", namespace)
    try:
        initial = {
            'options': {OPTION: 100},
            'generated_at': '2020-01-01T00:00:00Z',
        }
        apply(
            {
                'apiVersion': 'v1',
                'kind': 'ConfigMap',
                'metadata': {'name': CONFIGMAP, 'namespace': namespace},
                'data': {'values.json': json.dumps(initial, separators=(',', ':'))},
            },
        )
        apply(
            {
                'apiVersion': 'v1',
                'kind': 'ConfigMap',
                'metadata': {'name': FEATURES_CONFIGMAP, 'namespace': namespace},
                'data': {
                    'values.json': json.dumps(
                        {'options': {}, 'generated_at': initial['generated_at']},
                        separators=(',', ':'),
                    ),
                },
            },
        )
        apply(
            {
                'apiVersion': 'v1',
                'kind': 'ServiceAccount',
                'metadata': {'name': POD, 'namespace': namespace},
            },
        )
        apply(
            {
                'apiVersion': 'rbac.authorization.k8s.io/v1',
                'kind': 'Role',
                'metadata': {'name': POD, 'namespace': namespace},
                'rules': [
                    {
                        "apiGroups": [""],
                        "resources": ["configmaps"],
                        "resourceNames": [CONFIGMAP],
                        "verbs": ["patch"],
                    },
                    {
                        "apiGroups": [""],
                        "resources": ["pods"],
                        "resourceNames": [POD],
                        "verbs": ["patch"],
                    },
                ],
            },
        )
        apply(
            {
                'apiVersion': 'rbac.authorization.k8s.io/v1',
                'kind': 'RoleBinding',
                'metadata': {'name': POD, 'namespace': namespace},
                'subjects': [
                    {'kind': 'ServiceAccount', 'name': POD, 'namespace': namespace},
                ],
                'roleRef': {
                    'apiGroup': 'rbac.authorization.k8s.io',
                    'kind': 'Role',
                    'name': POD,
                },
            },
        )
        # Minikube has no options admission webhook. Materialize the volumes
        # that production's options.sentry.io injector adds for Getsentry pods.
        apply(
            {
                'apiVersion': 'v1',
                'kind': 'Pod',
                'metadata': {
                    'name': POD,
                    'namespace': namespace,
                    'annotations': {
                        'options.sentry.io/inject': 'true',
                        'options.sentry.io/namespace': 'getsentry,getsentry-features',
                    },
                },
                'spec': {
                    'restartPolicy': 'Never',
                    'serviceAccountName': POD,
                    'containers': [
                        {
                            "name": "probe",
                            "image": image,
                            "imagePullPolicy": "Never",
                            "command": ["python", "-u", "/app/probe.py"],
                            "env": [
                                {"name": "SENTRY_OPTIONS_DIR", "value": "/etc/sentry-options"},
                                {"name": "SAMPLE_COUNT", "value": str(sample_count)},
                                {"name": "SAMPLE_TIMEOUT_SECONDS", "value": str(sample_timeout)},
                                {
                                    "name": "REQUEST_POD_REFRESH",
                                    "value": "true" if request_pod_refresh else "false",
                                },
                                {
                                    "name": "POD_NAME",
                                    "valueFrom": {"fieldRef": {"fieldPath": "metadata.name"}},
                                },
                            ],
                            'volumeMounts': [
                                {
                                    'name': CONFIGMAP,
                                    'mountPath': '/etc/sentry-options/values/getsentry',
                                    'readOnly': True,
                                },
                                {
                                    'name': FEATURES_CONFIGMAP,
                                    'mountPath': '/etc/sentry-options/values/getsentry-features',
                                    'readOnly': True,
                                },
                            ],
                        },
                    ],
                    'volumes': [
                        {
                            'name': CONFIGMAP,
                            'configMap': {'name': CONFIGMAP, 'optional': True},
                        },
                        {
                            'name': FEATURES_CONFIGMAP,
                            'configMap': {'name': FEATURES_CONFIGMAP, 'optional': True},
                        },
                    ],
                },
            },
        )
        wait_for_event(namespace, 'ready', 120)
        found = wait_for_event(namespace, 'done', sample_count * sample_timeout + 30)
        samples = [event for event in found if event['event'] == 'sample']
        if [event['iteration'] for event in samples] != list(range(1, sample_count + 1)):
            raise RuntimeError(f'Expected {sample_count} ordered samples, got {samples}')
        latencies = [float(event['latency_seconds']) for event in samples]
        if any(not math.isfinite(latency) or latency < 0 for latency in latencies):
            raise RuntimeError(f'Invalid latency samples: {latencies}')

        update_label = 'update' if sample_count == 1 else 'updates'
        print(f'Minikube ConfigMap propagation latency ({sample_count} {update_label})')
        print(
            'Measured from time.monotonic() immediately before the pod sends a '
            'Kubernetes server-side apply PATCH until the first new-first '
            f'dual-read of {OPTION} returns that update in the same running process.',
        )
        print(
            f"Each dual-read calls options('{OPTIONS_NAMESPACE}').isset('{OPTION}'), "
            'then get() when set; otherwise it returns a simulated legacy value '
            'of 5. Includes API processing, kubelet volume projection, and the '
            "client's lazy refresh. The probe reads every 100 ms.",
        )
        print(
            f'Excludes image build and pod startup. '
            f'Each update flips {OPTION} between 100 and 101.',
        )
        if request_pod_refresh:
            print(
                "After each successful ConfigMap PATCH, the pod starts one "
                "best-effort PATCH of its own options.sentry.io/refresh-requested-at "
                "annotation in a background thread. Client polling does not wait "
                "for this request; its effect, when successful, is included in "
                "the measured interval."
            )
            refreshes = {
                event["iteration"]: event
                for event in found
                if event["event"] == "pod_refresh"
            }
            print("Run  PATCH to dual-read (s)  ConfigMap API (s)  Pod refresh")
        else:
            print("Run  PATCH to dual-read (s)  ConfigMap API (s)")
        for event in samples:
            row = (
                f"{event['iteration']:>3}  {event['latency_seconds']:>22.3f}  "
                f"{event['api_patch_seconds']:>17.3f}"
            )
            if request_pod_refresh:
                refresh = refreshes.get(event["iteration"])
                row += f"  {refresh['status'] if refresh else 'unconfirmed':>11}"
            print(row)
        print(f"Mean: {statistics.mean(latencies):.3f}s")
        if sample_count > 1:
            print(f'Sample standard deviation: {statistics.stdev(latencies):.3f}s')
        else:
            print('Sample standard deviation: n/a (one sample)')
    finally:
        kubectl('delete', 'namespace', namespace, '--wait=false')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default=IMAGE)
    parser.add_argument("--samples", type=int, default=DEFAULT_SAMPLE_COUNT)
    parser.add_argument("--sample-timeout", type=float, default=240.0)
    parser.add_argument("--request-pod-refresh", choices=("true", "false"), default="false")
    args = parser.parse_args()
    if args.samples < 1:
        parser.error('--samples must be at least 1')
    if not math.isfinite(args.sample_timeout) or args.sample_timeout <= 0:
        parser.error('--sample-timeout must be a positive finite number')
    try:
        run(args.image, args.samples, args.sample_timeout, args.request_pod_refresh == "true")
    except (RuntimeError, TimeoutError, FileNotFoundError) as exc:
        print(exc, file=sys.stderr)
        raise SystemExit(1) from exc
