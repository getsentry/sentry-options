.PHONY: test-minikube-propagation-latency

PROPAGATION_SAMPLES ?= 1

test-minikube-propagation-latency:
	minikube start --keep-context
	docker build -f examples/minikube_propagation/Dockerfile -t sentry-options-propagation:local .
	minikube image load sentry-options-propagation:local
	python3 examples/minikube_propagation/driver.py --samples "$(PROPAGATION_SAMPLES)"
