"""Report in-pod Kubernetes PATCH request to client get latency.

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


CONTEXT = "minikube"
CONFIGMAP = "sentry-options-sentry-options-testing"
POD = "sentry-options-latency"
OPTIONS_NAMESPACE = "sentry-options-testing"
IMAGE = "sentry-options-propagation:local"
DEFAULT_SAMPLE_COUNT = 1


def kubectl(*args: str, input_text: str | None = None) -> str:
    result = subprocess.run(
        ["kubectl", "--context", CONTEXT, *args],
        input=input_text,
        text=True,
        capture_output=True,
        check=False,
    )
    if result.returncode:
        raise RuntimeError(f"kubectl {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout


def apply(resource: dict[str, Any]) -> None:
    kubectl("apply", "-f", "-", input_text=json.dumps(resource))


def events(namespace: str) -> list[dict[str, Any]]:
    logs = kubectl("-n", namespace, "logs", POD)
    found = []
    for line in logs.splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(event, dict) and isinstance(event.get("event"), str):
            found.append(event)
    return found


def wait_for_event(namespace: str, name: str, timeout: float) -> list[dict[str, Any]]:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            found = events(namespace)
        except RuntimeError:
            found = []
        if any(event["event"] == name for event in found):
            return found
        time.sleep(0.5)
    status = kubectl("-n", namespace, "get", "pod", POD, "-o", "json")
    logs = kubectl("-n", namespace, "logs", POD)
    raise TimeoutError(f"Timed out waiting for {name}. Pod status: {status}. Logs: {logs}")


def run(image: str, sample_count: int, sample_timeout: float) -> None:
    namespace = f"sentry-options-latency-{uuid.uuid4().hex[:8]}"
    kubectl("create", "namespace", namespace)
    try:
        initial = {
            "options": {"bool-option": False},
            "generated_at": "2020-01-01T00:00:00Z",
        }
        apply(
            {
                "apiVersion": "v1",
                "kind": "ConfigMap",
                "metadata": {"name": CONFIGMAP, "namespace": namespace},
                "data": {"values.json": json.dumps(initial)},
            }
        )
        apply(
            {
                "apiVersion": "v1",
                "kind": "ServiceAccount",
                "metadata": {"name": POD, "namespace": namespace},
            }
        )
        apply(
            {
                "apiVersion": "rbac.authorization.k8s.io/v1",
                "kind": "Role",
                "metadata": {"name": POD, "namespace": namespace},
                "rules": [
                    {
                        "apiGroups": [""],
                        "resources": ["configmaps"],
                        "resourceNames": [CONFIGMAP],
                        "verbs": ["patch"],
                    }
                ],
            }
        )
        apply(
            {
                "apiVersion": "rbac.authorization.k8s.io/v1",
                "kind": "RoleBinding",
                "metadata": {"name": POD, "namespace": namespace},
                "subjects": [
                    {"kind": "ServiceAccount", "name": POD, "namespace": namespace}
                ],
                "roleRef": {
                    "apiGroup": "rbac.authorization.k8s.io",
                    "kind": "Role",
                    "name": POD,
                },
            }
        )
        apply(
            {
                "apiVersion": "v1",
                "kind": "Pod",
                "metadata": {"name": POD, "namespace": namespace},
                "spec": {
                    "restartPolicy": "Never",
                    "serviceAccountName": POD,
                    "containers": [
                        {
                            "name": "probe",
                            "image": image,
                            "imagePullPolicy": "Never",
                            "command": ["python", "-u", "/app/probe.py"],
                            "env": [
                                {"name": "SENTRY_OPTIONS_DIR", "value": "/etc/sentry-options"},
                                {"name": "SAMPLE_COUNT", "value": str(sample_count)},
                                {"name": "SAMPLE_TIMEOUT_SECONDS", "value": str(sample_timeout)},
                            ],
                            "volumeMounts": [
                                {
                                    "name": "values",
                                    "mountPath": "/etc/sentry-options/values/sentry-options-testing",
                                    "readOnly": True,
                                }
                            ],
                        }
                    ],
                    "volumes": [{"name": "values", "configMap": {"name": CONFIGMAP}}],
                },
            }
        )
        wait_for_event(namespace, "ready", 120)
        found = wait_for_event(namespace, "done", sample_count * sample_timeout + 30)
        samples = [event for event in found if event["event"] == "sample"]
        if [event["iteration"] for event in samples] != list(range(1, sample_count + 1)):
            raise RuntimeError(f"Expected {sample_count} ordered samples, got {samples}")
        latencies = [float(event["latency_seconds"]) for event in samples]
        if any(not math.isfinite(latency) or latency < 0 for latency in latencies):
            raise RuntimeError(f"Invalid latency samples: {latencies}")

        update_label = "update" if sample_count == 1 else "updates"
        print(f"Minikube ConfigMap propagation latency ({sample_count} {update_label})")
        print(
            "Measured from time.monotonic() immediately before the pod sends a "
            "Kubernetes API PATCH request until the first normal "
            f"sentry_options.options('{OPTIONS_NAMESPACE}').get('bool-option') "
            "returns that update in the same running process."
        )
        print(
            "Includes API processing, kubelet volume projection, and the "
            "client's lazy refresh. The probe reads every 100 ms."
        )
        print("Excludes image build and pod startup. Each update flips bool-option.")
        print("Run  PATCH to get (s)  API request (s)")
        for event in samples:
            print(
                f"{event['iteration']:>3}  {event['latency_seconds']:>16.3f}  "
                f"{event['api_patch_seconds']:>15.3f}"
            )
        print(f"Mean: {statistics.mean(latencies):.3f}s")
        if sample_count > 1:
            print(f"Sample standard deviation: {statistics.stdev(latencies):.3f}s")
        else:
            print("Sample standard deviation: n/a (one sample)")
    finally:
        kubectl("delete", "namespace", namespace, "--wait=false")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", default=IMAGE)
    parser.add_argument("--samples", type=int, default=DEFAULT_SAMPLE_COUNT)
    parser.add_argument("--sample-timeout", type=float, default=240.0)
    args = parser.parse_args()
    if args.samples < 1:
        parser.error("--samples must be at least 1")
    if not math.isfinite(args.sample_timeout) or args.sample_timeout <= 0:
        parser.error("--sample-timeout must be a positive finite number")
    try:
        run(args.image, args.samples, args.sample_timeout)
    except (RuntimeError, TimeoutError, FileNotFoundError) as exc:
        print(exc, file=sys.stderr)
        raise SystemExit(1) from exc
