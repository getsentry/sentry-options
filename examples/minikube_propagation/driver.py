"""Report ConfigMap apply to client read latency through the real injector.

Run with `make test-minikube-propagation-latency` (kubelet ConfigMap volumes)
or `make test-minikube-propagation-latency-sidecar` (sentry-options-sync
sidecar). This is an opt-in integration probe, not a CI latency gate.
"""
from __future__ import annotations

import argparse
import base64
import json
import math
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
import uuid
from pathlib import Path
from typing import Any


CONTEXT = 'minikube'
HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]
CLI = REPO / 'target' / 'debug' / 'sentry-options-cli'
TARGET = 'us'
OPTIONS_NAMESPACES = ('getsentry', 'getsentry-features')
OPTION = 'getsentry.options-dual-read-test'
INITIAL_VALUE = 100
UPDATED_VALUE = 101
APP = 'getsentry-web'
SERVICE_ACCOUNT = 'service-getsentry'
INJECTOR = 'sentry-envoy-injector'
DEFAULT_SAMPLE_COUNT = 5


def kubectl(*args: str, input_text: str | None = None, check: bool = True) -> str:
    result = subprocess.run(
        ['kubectl', '--context', CONTEXT, *args],
        input=input_text,
        text=True,
        capture_output=True,
        check=False,
    )
    if check and result.returncode:
        raise RuntimeError(f"kubectl {' '.join(args)} failed: {result.stderr.strip()}")
    return result.stdout


def apply(*resources: dict[str, Any]) -> None:
    kubectl('apply', '-f', '-', input_text='\n---\n'.join(json.dumps(r) for r in resources))


class Values:
    """An automator-style checkout: option-values/{ns}/{target} plus fetched schemas."""

    def __init__(self, root: Path) -> None:
        self.schemas = root / 'schemas'
        self.option_values = root / 'option-values'
        shutil.copytree(HERE / 'schemas', self.schemas)
        for ns in OPTIONS_NAMESPACES:
            self.write(ns, 'default', {})
            self.write(ns, TARGET, {})

    def write(self, ns: str, target: str, options: dict[str, Any]) -> None:
        path = self.option_values / ns / target / 'values.yaml'
        path.parent.mkdir(parents=True, exist_ok=True)
        # JSON is valid YAML.
        path.write_text(json.dumps({'options': options}))

    def configmap(self, ns: str) -> str:
        """Generate the ConfigMap exactly as deploy-new-sentry-options.sh does."""
        result = subprocess.run(
            [
                str(CLI), 'write', '--quiet',
                '--schemas', str(self.schemas),
                '--root', str(self.option_values),
                '--output-format', 'configmap',
                '--namespace', ns,
                '--target', TARGET,
                '--commit-sha', uuid.uuid4().hex,
                '--commit-timestamp', str(int(time.time())),
            ],
            text=True,
            capture_output=True,
            check=True,
        )
        return result.stdout


def deploy_injector(namespace: str, image: str, workdir: Path) -> str:
    """Run the envoy-injector in-cluster behind TLS; return its CA bundle.

    Production runs the injector on Cloud Run behind a TLS load balancer that
    the webhook reaches through a Service. Here an nginx sidecar terminates TLS
    in front of the same plain-HTTP injector binary.
    """
    host = f'{INJECTOR}.{namespace}.svc'
    key, cert = workdir / 'tls.key', workdir / 'tls.crt'
    subprocess.run(
        [
            'openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-days', '1',
            '-keyout', str(key), '-out', str(cert), '-subj', f'/CN={host}',
            '-addext', f'subjectAltName=DNS:{host}',
        ],
        check=True,
        capture_output=True,
    )
    labels = {'app': INJECTOR}
    apply(
        {
            'apiVersion': 'v1',
            'kind': 'Secret',
            'metadata': {'name': f'{INJECTOR}-tls', 'namespace': namespace},
            'type': 'kubernetes.io/tls',
            'stringData': {'tls.crt': cert.read_text(), 'tls.key': key.read_text()},
        },
        {
            'apiVersion': 'v1',
            'kind': 'ConfigMap',
            'metadata': {'name': f'{INJECTOR}-nginx', 'namespace': namespace},
            'data': {
                'nginx.conf': (
                    'events {}\n'
                    'http { server {\n'
                    '  listen 8443 ssl;\n'
                    '  ssl_certificate /tls/tls.crt;\n'
                    '  ssl_certificate_key /tls/tls.key;\n'
                    '  location / { proxy_pass http://127.0.0.1:8080; }\n'
                    '} }\n'
                ),
            },
        },
        {
            'apiVersion': 'apps/v1',
            'kind': 'Deployment',
            'metadata': {'name': INJECTOR, 'namespace': namespace},
            'spec': {
                'replicas': 1,
                'selector': {'matchLabels': labels},
                'template': {
                    'metadata': {'labels': labels},
                    'spec': {
                        'containers': [
                            {
                                'name': 'injector',
                                'image': image,
                                'imagePullPolicy': 'Never',
                                'env': [
                                    {'name': 'SERVICE_PORT', 'value': '8080'},
                                    {'name': 'PROJECT_NUMBER', 'value': '0'},
                                    {'name': 'MESH_NAME', 'value': 'minikube'},
                                ],
                                'readinessProbe': {
                                    'httpGet': {'path': '/health', 'port': 8080},
                                    'periodSeconds': 1,
                                },
                            },
                            {
                                'name': 'tls',
                                'image': 'nginx:1.27-alpine',
                                'volumeMounts': [
                                    {'name': 'tls', 'mountPath': '/tls', 'readOnly': True},
                                    {
                                        'name': 'nginx',
                                        'mountPath': '/etc/nginx/nginx.conf',
                                        'subPath': 'nginx.conf',
                                        'readOnly': True,
                                    },
                                ],
                            },
                        ],
                        'volumes': [
                            {'name': 'tls', 'secret': {'secretName': f'{INJECTOR}-tls'}},
                            {'name': 'nginx', 'configMap': {'name': f'{INJECTOR}-nginx'}},
                        ],
                    },
                },
            },
        },
        {
            'apiVersion': 'v1',
            'kind': 'Service',
            'metadata': {'name': INJECTOR, 'namespace': namespace},
            'spec': {'selector': labels, 'ports': [{'port': 443, 'targetPort': 8443}]},
        },
    )
    kubectl('-n', namespace, 'rollout', 'status', f'deployment/{INJECTOR}', '--timeout=180s')
    return base64.b64encode(cert.read_bytes()).decode()


def webhook(name: str, namespace: str, ca_bundle: str, run_id: str) -> dict[str, Any]:
    """Mirror ops k8s/services/envoy-injector/webhook.yaml.

    The namespace selector also matches this run's label so a leftover webhook
    from an interrupted run cannot affect other namespaces.
    """
    return {
        'apiVersion': 'admissionregistration.k8s.io/v1',
        'kind': 'MutatingWebhookConfiguration',
        'metadata': {'name': name},
        'webhooks': [
            {
                'name': 'sentry-envoy-injector.sentry.io',
                'sideEffects': 'None',
                'admissionReviewVersions': ['v1beta1'],
                'clientConfig': {
                    'service': {
                        'name': INJECTOR,
                        'namespace': namespace,
                        'path': '/mutate',
                        'port': 443,
                    },
                    'caBundle': ca_bundle,
                },
                'rules': [
                    {
                        'operations': ['CREATE'],
                        'apiGroups': [''],
                        'apiVersions': ['v1'],
                        'resources': ['pods'],
                    },
                ],
                'failurePolicy': 'Fail',
                'namespaceSelector': {
                    'matchLabels': {
                        'sentry-envoy-injection': 'enabled',
                        'sentry-options-latency-run': run_id,
                    },
                },
            },
        ],
    }


def app_resources(
    namespace: str, image: str, sidecar: bool, sync_image: str,
) -> list[dict[str, Any]]:
    """A Getsentry-like Deployment plus the RBAC the sync sidecar needs."""
    annotations = {
        'options.sentry.io/inject': 'true',
        'options.sentry.io/namespace': ','.join(OPTIONS_NAMESPACES),
    }
    resources: list[dict[str, Any]] = [
        {
            'apiVersion': 'v1',
            'kind': 'ServiceAccount',
            'metadata': {'name': SERVICE_ACCOUNT, 'namespace': namespace},
        },
    ]
    if sidecar:
        annotations['options.sentry.io/delivery'] = 'sidecar'
        annotations['options.sentry.io/syncImage'] = sync_image
        resources += [
            {
                'apiVersion': 'rbac.authorization.k8s.io/v1',
                'kind': 'Role',
                'metadata': {'name': 'sentry-options-sync', 'namespace': namespace},
                'rules': [
                    {
                        'apiGroups': [''],
                        'resources': ['configmaps'],
                        'resourceNames': [f'sentry-options-{ns}' for ns in OPTIONS_NAMESPACES],
                        'verbs': ['get', 'list', 'watch'],
                    },
                ],
            },
            {
                'apiVersion': 'rbac.authorization.k8s.io/v1',
                'kind': 'RoleBinding',
                'metadata': {'name': 'sentry-options-sync', 'namespace': namespace},
                'subjects': [
                    {'kind': 'ServiceAccount', 'name': SERVICE_ACCOUNT, 'namespace': namespace},
                ],
                'roleRef': {
                    'apiGroup': 'rbac.authorization.k8s.io',
                    'kind': 'Role',
                    'name': 'sentry-options-sync',
                },
            },
        ]
    labels = {'app': APP}
    resources.append(
        {
            'apiVersion': 'apps/v1',
            'kind': 'Deployment',
            'metadata': {'name': APP, 'namespace': namespace},
            'spec': {
                'replicas': 1,
                'selector': {'matchLabels': labels},
                'template': {
                    'metadata': {'labels': labels, 'annotations': annotations},
                    'spec': {
                        'serviceAccountName': SERVICE_ACCOUNT,
                        'containers': [
                            {
                                'name': 'web',
                                'image': image,
                                'imagePullPolicy': 'Never',
                                'command': ['python', '-u', '/app/probe.py'],
                            },
                        ],
                    },
                },
            },
        },
    )
    return resources


def app_pod(namespace: str) -> dict[str, Any]:
    pods = json.loads(kubectl('-n', namespace, 'get', 'pods', '-l', f'app={APP}', '-o', 'json'))
    running = [p for p in pods['items'] if not p['metadata'].get('deletionTimestamp')]
    if len(running) != 1:
        raise RuntimeError(f'Expected one {APP} pod, found {len(running)}')
    return running[0]


def observations(namespace: str, pod: str) -> list[dict[str, Any]]:
    found = []
    for line in kubectl('-n', namespace, 'logs', pod, '-c', 'web').splitlines():
        try:
            event = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(event, dict) and event.get('event') == 'observed':
            found.append(event)
    return found


def clock_offset(namespace: str, pod: str) -> tuple[float, float]:
    """Estimate pod clock minus host clock, and its uncertainty.

    Values are applied from the host, as GoCD applies them from outside the
    cluster, but the pod timestamps their arrival. Bracketing a pod clock read
    between two host reads bounds the offset by half the round trip.
    """
    best: tuple[float, float] | None = None
    for _ in range(5):
        before = time.time()
        pod_time = float(
            kubectl('-n', namespace, 'exec', pod, '-c', 'web', '--', 'python', '-c',
                    'import time; print(time.time())'),
        )
        after = time.time()
        estimate = (pod_time - (before + after) / 2, (after - before) / 2)
        if best is None or estimate[1] < best[1]:
            best = estimate
    assert best is not None
    return best


def check_permissions(namespace: str, sidecar: bool) -> list[str]:
    """Verify the app identity can read only its option ConfigMaps."""
    user = f'system:serviceaccount:{namespace}:{SERVICE_ACCOUNT}'
    checks = [
        ('watch', f'configmaps/sentry-options-{OPTIONS_NAMESPACES[0]}', sidecar),
        ('list', 'configmaps', False),
        ('patch', f'configmaps/sentry-options-{OPTIONS_NAMESPACES[0]}', False),
        ('patch', 'pods', False),
    ]
    lines = []
    for verb, resource, expected in checks:
        allowed = kubectl(
            '-n', namespace, 'auth', 'can-i', verb, resource, '--as', user, check=False,
        ).strip() == 'yes'
        if allowed != expected:
            raise RuntimeError(f'{SERVICE_ACCOUNT} {verb} {resource}: allowed={allowed}')
        lines.append(f"  {verb} {resource}: {'yes' if allowed else 'no'}")
    return lines


def describe_injection(pod: dict[str, Any], sidecar: bool) -> list[str]:
    """Confirm the webhook, not this driver, added the options wiring."""
    spec = pod['spec']
    lines = []
    for ns in OPTIONS_NAMESPACES:
        name = f'sentry-options-{ns}'
        volume = next((v for v in spec.get('volumes', []) if v['name'] == name), None)
        if volume is None:
            raise RuntimeError(f'Injector did not add volume {name}')
        source = 'emptyDir' if 'emptyDir' in volume else 'configMap' if 'configMap' in volume else '?'
        if (source == 'emptyDir') != sidecar:
            raise RuntimeError(f'Unexpected volume source {source} for {name}')
        lines.append(f'  volume {name}: {source} at /etc/sentry-options/values/{ns}')
    syncs = [c for c in spec.get('initContainers', []) if c['name'] == 'sentry-options-sync']
    if bool(syncs) != sidecar:
        raise RuntimeError(f'Unexpected sentry-options-sync containers: {syncs}')
    if syncs:
        lines.append(
            f"  native sidecar sentry-options-sync (restartPolicy={syncs[0].get('restartPolicy')})",
        )
    return lines


def wait_for_value(
    namespace: str, pod: str, value: int, after: float, offset: float, timeout: float,
) -> float:
    """Return the host-clock time the client first observed `value` after `after`."""
    deadline = time.time() + timeout
    while time.time() < deadline:
        for event in observations(namespace, pod):
            observed = event['wall'] - offset
            if event['value'] == value and observed >= after:
                return observed
        time.sleep(0.5)
    raise TimeoutError(f'{OPTION}={value} did not reach the client within {timeout}s')


def run(args: argparse.Namespace) -> None:
    sidecar = args.delivery == 'sidecar'
    run_id = uuid.uuid4().hex[:8]
    infra_ns = f'sentry-options-injector-{run_id}'
    app_ns = f'sentry-options-latency-{run_id}'
    webhook_name = f'sentry-envoy-injector-{run_id}.sentry.io'

    subprocess.run(['cargo', 'build', '-q', '-p', 'sentry-options-cli'], cwd=REPO, check=True)
    with tempfile.TemporaryDirectory() as tmp:
        workdir = Path(tmp)
        values = Values(workdir)
        values.write('getsentry', 'default', {OPTION: INITIAL_VALUE})
        try:
            kubectl('create', 'namespace', infra_ns)
            apply(
                {
                    'apiVersion': 'v1',
                    'kind': 'Namespace',
                    'metadata': {
                        'name': app_ns,
                        'labels': {
                            'sentry-envoy-injection': 'enabled',
                            'sentry-options-latency-run': run_id,
                        },
                    },
                },
            )
            ca_bundle = deploy_injector(infra_ns, args.injector_image, workdir)
            apply(webhook(webhook_name, infra_ns, ca_bundle, run_id))

            for ns in OPTIONS_NAMESPACES:
                kubectl(
                    '-n', app_ns, 'apply', '--server-side', '--force-conflicts', '-f', '-',
                    input_text=values.configmap(ns),
                )
            apply(*app_resources(app_ns, args.image, sidecar, args.sync_image))
            kubectl('-n', app_ns, 'rollout', 'status', f'deployment/{APP}', '--timeout=180s')
            pod = app_pod(app_ns)
            pod_name = pod['metadata']['name']
            injection = describe_injection(pod, sidecar)
            permissions = check_permissions(app_ns, sidecar)

            # The first read must already see the ConfigMap value: kubelet mounts
            # volumes before start, and the sidecar's startup probe gates the app.
            deadline = time.time() + 60
            while not (seen := observations(app_ns, pod_name)) and time.time() < deadline:
                time.sleep(0.5)
            if not seen or seen[0]['value'] != INITIAL_VALUE:
                raise RuntimeError(f'Expected first read {INITIAL_VALUE}, got {seen[:1]}')

            offset, uncertainty = clock_offset(app_ns, pod_name)
            samples = []
            for iteration in range(1, args.samples + 1):
                value = UPDATED_VALUE if iteration % 2 else INITIAL_VALUE
                values.write('getsentry', 'default', {OPTION: value})
                configmap = values.configmap('getsentry')
                started = time.time()
                kubectl(
                    '-n', app_ns, 'apply', '--server-side', '--force-conflicts', '-f', '-',
                    input_text=configmap,
                )
                applied = time.time()
                observed = wait_for_value(
                    app_ns, pod_name, value, started, offset, args.sample_timeout,
                )
                samples.append((observed - started, applied - started))
        finally:
            kubectl('delete', 'mutatingwebhookconfiguration', webhook_name, '--ignore-not-found', check=False)
            kubectl('delete', 'namespace', app_ns, infra_ns, '--wait=false', '--ignore-not-found', check=False)

    latencies = [latency for latency, _ in samples]
    if any(not math.isfinite(latency) or latency < 0 for latency in latencies):
        raise RuntimeError(f'Invalid latency samples: {latencies}')
    label = 'sync sidecar' if sidecar else 'kubelet ConfigMap volume'
    print(f'Minikube options propagation latency: {label} ({len(samples)} updates)')
    print('Pod injected by sentry-envoy-injector through its MutatingWebhookConfiguration:')
    print('\n'.join(injection))
    print(f'{SERVICE_ACCOUNT} permissions:')
    print('\n'.join(permissions))
    print(
        'Each update regenerates the ConfigMap with sentry-options-cli and applies it '
        'with kubectl apply --server-side --force-conflicts from the host, like '
        'deploy-new-sentry-options.sh. Latency runs from just before that apply to '
        f"the app's first new-first dual-read of {OPTION} returning the new value "
        '(read every 100 ms, default 5 s client refresh threshold).',
    )
    print(f'Pod clock offset {offset:+.3f}s, uncertainty ±{uncertainty:.3f}s.')
    print('Run  Apply to dual-read (s)  kubectl apply (s)')
    for i, (latency, apply_seconds) in enumerate(samples, 1):
        print(f'{i:>3}  {latency:>22.3f}  {apply_seconds:>17.3f}')
    print(f'Mean: {statistics.mean(latencies):.3f}s')
    if len(latencies) > 1:
        print(f'Sample standard deviation: {statistics.stdev(latencies):.3f}s')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--delivery', choices=('configmap', 'sidecar'), default='configmap')
    parser.add_argument('--image', default='sentry-options-propagation:local')
    parser.add_argument('--sync-image', default='sentry-options-sync:local')
    parser.add_argument('--injector-image', default='sentry-envoy-injector:local')
    parser.add_argument('--samples', type=int, default=DEFAULT_SAMPLE_COUNT)
    parser.add_argument('--sample-timeout', type=float, default=240.0)
    args = parser.parse_args()
    if args.samples < 1:
        parser.error('--samples must be at least 1')
    if not math.isfinite(args.sample_timeout) or args.sample_timeout <= 0:
        parser.error('--sample-timeout must be a positive finite number')
    try:
        run(args)
    except (RuntimeError, TimeoutError, subprocess.CalledProcessError) as exc:
        print(exc, file=sys.stderr)
        raise SystemExit(1) from exc
