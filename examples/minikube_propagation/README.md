# Local ConfigMap propagation latency

From the repository root on macOS, run:

```sh
make test-minikube-propagation-latency
```

The target starts Minikube, builds the Python client from this checkout into a
local image, loads it into Minikube, and runs `driver.py`. Docker and Minikube
must be installed. The driver always uses the `minikube` kubectl context and
creates a temporary namespace. It removes that namespace when finished.

The pod runs `probe.py` throughout the measurement. The default is one update;
to collect ten samples, run:

```sh
make test-minikube-propagation-latency PROPAGATION_SAMPLES=10
```

For each update, the pod records `time.monotonic()` immediately before sending
a Kubernetes API PATCH request for the mounted ConfigMap. It stops the timer
immediately after the first normal
`sentry_options.options("sentry-options-testing").get("bool-option")`
call returns that update in the same process. The probe checks every 100 ms and
flips the boolean each time, so every sample requires a new value to reach the
client.

The printed PATCH-to-get latency includes API processing, kubelet volume
projection, and the client's lazy refresh. It excludes image build and pod
startup. The report lists each sample and the mean. With at least two
samples, it also prints the sample standard deviation. No fixed latency limit
is asserted because kubelet timing varies. This measures the local client and
Kubernetes path, without Getsentry's legacy option hook.
