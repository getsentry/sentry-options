# Local ConfigMap propagation latency

From the repository root on macOS, run:

```sh
make test-minikube-propagation-latency
```

The target starts Minikube, builds the Python client from this checkout into a
local image, loads it into Minikube, and runs `driver.py`. Docker and Minikube
must be installed. The driver always uses the `minikube` kubectl context and
creates a temporary namespace. It removes that namespace when finished.

The pod mirrors the Getsentry options mounts configured in `ops`: optional,
read-only ConfigMaps `sentry-options-getsentry` and
`sentry-options-getsentry-features` at `/etc/sentry-options/values/<namespace>`.
It carries the same `options.sentry.io` annotations as a Getsentry pod. Minikube
does not run the production admission webhook, so `driver.py` creates those
volumes directly. The bundled schema contains the production integer type and
default for `getsentry.options-dual-read-test`; the ConfigMap starts at 100 and
the probe alternates it between 101 and 100. The synthetic ConfigMaps are much
smaller than production's.

The pod runs `probe.py` throughout the measurement. The default is five updates;
to collect ten samples, run:

```sh
make test-minikube-propagation-latency PROPAGATION_SAMPLES=10
```

For each update, the pod records `time.monotonic()` immediately before sending
a Kubernetes server-side apply PATCH for the mounted ConfigMap, matching the
production deploy's update method. It stops the timer immediately after the
first new-options-first dual-read returns that update in the same process. Like
the Getsentry read hook, that read calls `options("getsentry")`, checks
`isset("getsentry.options-dual-read-test")`, and calls `get()` only when the value
is set. The current Getsentry hook checks its legacy store first; this probe
reverses that priority and returns a simulated legacy value of 5 only when the
new value is unavailable. The sampled option is registered as automator
modifiable in Getsentry. The probe checks every 100 ms; every sample requires a
new value to reach the client.

The printed PATCH-to-dual-read latency includes API processing, kubelet volume
projection, and the client's lazy refresh. It excludes image build and pod
startup. The report lists each sample and the mean. With at least two
samples, it also prints the sample standard deviation. No fixed latency limit
is asserted because kubelet timing varies. This models the new-first branch of
Getsentry's hook with a fixed legacy fallback, without loading the Sentry
options manager or its store. Minikube's kubelet configuration and Kubernetes
version may differ from production GKE.
