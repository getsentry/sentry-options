.PHONY: test-minikube-propagation-latency test-minikube-propagation-latency-annotation

PROPAGATION_SAMPLES ?= 5
PROPAGATION_REQUEST_POD_REFRESH ?= false

test-minikube-propagation-latency:
	minikube start --keep-context
	docker build -f examples/minikube_propagation/Dockerfile -t sentry-options-propagation:local .
	minikube image load sentry-options-propagation:local
	python3 examples/minikube_propagation/driver.py --samples "$(PROPAGATION_SAMPLES)" --request-pod-refresh "$(PROPAGATION_REQUEST_POD_REFRESH)"

test-minikube-propagation-latency-annotation: PROPAGATION_REQUEST_POD_REFRESH = true
test-minikube-propagation-latency-annotation: test-minikube-propagation-latency
