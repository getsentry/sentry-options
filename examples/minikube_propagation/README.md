# Local ConfigMap propagation latency

From the repository root on macOS, with Docker and Minikube installed and
`sentry-envoy-injector` checked out next to this repository, run:

```sh
make test-minikube-propagation-latency          # kubelet ConfigMap volumes
make test-minikube-propagation-latency-sidecar  # sentry-options-sync sidecar
```

Set `INJECTOR_DIR` if the injector checkout lives elsewhere, and
`PROPAGATION_SAMPLES` (default 5) to change the number of updates.

The target starts Minikube and builds three local images: the Python client
with `probe.py`, `sentry-options-sync`, and `sentry-envoy-injector`. It loads
them into Minikube and runs `driver.py`. The driver always uses the `minikube`
kubectl context. It creates two temporary namespaces and a
`MutatingWebhookConfiguration`, and removes them when finished.

The setup follows `docs/architecture.md`:

- **Injector.** The real `sentry-envoy-injector` binary runs in-cluster, with an
  nginx sidecar terminating TLS in front of it. In production, Cloud Run sits
  behind a TLS load balancer that the webhook reaches through a Service. The
  webhook mirrors ops `k8s/services/envoy-injector/webhook.yaml`: pod
  `CREATE`, `failurePolicy: Fail`, `admissionReviewVersions: [v1beta1]`, and a
  `sentry-envoy-injection: enabled` namespace selector. The selector also
  matches a per-run label, so a leftover webhook cannot affect other
  namespaces.
- **App.** A `getsentry-web` Deployment runs as `service-getsentry` with only
  the `options.sentry.io/inject` and `options.sentry.io/namespace:
  getsentry,getsentry-features` annotations. It carries no volumes of its own.
  The driver checks that the webhook added them. The image bakes the schemas
  into `/etc/sentry-options/schemas`, as a service image does.
- **Deploy.** Values live in an automator-style `option-values/{ns}/{default,us}`
  tree. Each update regenerates the ConfigMap with `sentry-options-cli write
  --output-format configmap` and applies it from the host with `kubectl apply
  --server-side --force-conflicts`, as `deploy-new-sentry-options.sh` does from
  GoCD.

In sidecar mode the Deployment also sets `options.sentry.io/delivery: sidecar`
and `options.sentry.io/syncImage`. The injector then mounts `emptyDir` volumes
and adds the `sentry-options-sync` native sidecar. The driver grants
`service-getsentry` `get`/`list`/`watch` on the two option ConfigMaps by name.
It uses `kubectl auth can-i` to confirm the identity can watch those
ConfigMaps but cannot list all ConfigMaps or patch ConfigMaps or Pods. In
ConfigMap mode, the identity has no ConfigMap access at all.

The app container runs `probe.py`. The probe never talks to Kubernetes. Every
100 ms it performs the Getsentry-style new-first dual-read of
`getsentry.options-dual-read-test`: `options("getsentry")`, then `isset()`,
then `get()` when set, else a simulated legacy value of 5. It logs each change
with its wall-clock time. The driver requires the first read to return the
ConfigMap's value, not the legacy fallback. That confirms kubelet mounts the
volume before start, and in sidecar mode that the startup probe held the app
until the first sync. Each update then flips the value between 100 and 101.
Latency runs from just before `kubectl apply` on the host to the first read
returning the new value. The driver converts pod timestamps to the host clock
with an offset measured through `kubectl exec`, and reports its uncertainty.

In sidecar mode the report also shows the sidecar's own
`sentry.options.sync.generation_to_write` measurement for each update, parsed
from its log: from the CLI's `generated_at`, just before the apply, to the
sidecar writing the values into the pod. In production the sidecar sends this
metric to the node's DogStatsD. The client's existing `propagation_delay` runs
from `generated_at` to its refresh, so the difference is the client's
refresh-on-read lag. Neither the app nor the automator changes.

The measurement includes API processing, volume projection (kubelet sync or
the sidecar's watch), and the client's refresh-on-read, which has a default 5 s
threshold. It excludes image builds and pod startup. No fixed latency limit is
asserted. Minikube's kubelet configuration and Kubernetes version may differ
from production GKE.
