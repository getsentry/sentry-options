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

The pod runs `probe.py` throughout the measurement. The default is one update;
to collect ten samples, run:

```sh
make test-minikube-propagation-latency PROPAGATION_SAMPLES=10
```

For each update, the pod records `time.monotonic()` immediately before sending
a Kubernetes server-side apply PATCH for the mounted ConfigMap, matching the
production deploy's update method. It stops the timer immediately after the
first normal
`sentry_options.options("getsentry").get("getsentry.options-dual-read-test")`
call returns that update in the same process. The probe checks every 100 ms;
every sample requires a new value to reach the client.

The printed PATCH-to-get latency includes API processing, kubelet volume
projection, and the client's lazy refresh. It excludes image build and pod
startup. The report lists each sample and the mean. With at least two
samples, it also prints the sample standard deviation. No fixed latency limit
is asserted because kubelet timing varies. This measures the local client and
Kubernetes path, without Getsentry's legacy option hook. Minikube's kubelet
configuration and Kubernetes version may differ from production GKE.
