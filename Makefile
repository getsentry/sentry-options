.PHONY: test-minikube-propagation-latency test-minikube-propagation-latency-sidecar

PROPAGATION_SAMPLES ?= 5
PROPAGATION_DELIVERY ?= configmap
INJECTOR_DIR ?= ../sentry-envoy-injector

test-minikube-propagation-latency:
	minikube start --keep-context
	docker build -f examples/minikube_propagation/Dockerfile -t sentry-options-propagation:local .
	docker build -f sentry-options-sync/Dockerfile -t sentry-options-sync:local .
	docker build -t sentry-envoy-injector:local "$(INJECTOR_DIR)"
	minikube image load sentry-options-propagation:local
	minikube image load sentry-options-sync:local
	minikube image load sentry-envoy-injector:local
	python3 examples/minikube_propagation/driver.py --samples "$(PROPAGATION_SAMPLES)" --delivery "$(PROPAGATION_DELIVERY)"

test-minikube-propagation-latency-sidecar: PROPAGATION_DELIVERY = sidecar
test-minikube-propagation-latency-sidecar: test-minikube-propagation-latency
