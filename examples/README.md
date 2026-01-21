# sentry-options Example Deployment

This guide walks through deploying the sentry-options example to a Kubernetes cluster.

## Overview

The deployment model:
- **Schemas** are baked into the Docker image at build time
- **Values** are mounted via Kubernetes ConfigMap at runtime
- The client watches for ConfigMap changes and hot-reloads values without pod restart

## Prerequisites

- Docker installed
- kubectl configured and connected to your cluster
- sentry-options-cli built: `cargo build --release -p sentry-options-cli`

## 1. Build Docker Image

```bash
# From the repo root
docker build -t sentry-options-example -f examples/Dockerfile .

# Tag for your registry (example with GCR)
docker tag sentry-options-example gcr.io/PROJECT_ID/sentry-options-example:latest

# Push to registry
docker push gcr.io/PROJECT_ID/sentry-options-example:latest
```

## 2. Generate and Apply ConfigMap

```bash
# Generate ConfigMap and apply directly to cluster
./target/release/sentry-options-cli write \
  --schemas examples/schemas \
  --root examples/configs \
  --output-format configmap \
  --namespace sentry-options-testing \
  --target default \
  | kubectl apply -f -
```

Or save to a file first:

```bash
./target/release/sentry-options-cli write \
  --schemas examples/schemas \
  --root examples/configs \
  --output-format configmap \
  --namespace sentry-options-testing \
  --target default \
  --out configmap.yaml

kubectl apply -f configmap.yaml
```

## 3. Deploy Example Pod

Create a deployment that mounts the ConfigMap:

```yaml
# deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: sentry-options-example
spec:
  replicas: 1
  selector:
    matchLabels:
      app: sentry-options-example
  template:
    metadata:
      labels:
        app: sentry-options-example
    spec:
      containers:
      - name: app
        image: gcr.io/PROJECT_ID/sentry-options-example:latest
        env:
        - name: SENTRY_OPTIONS_DIR
          value: /etc/sentry-options
        volumeMounts:
        - name: options-values
          mountPath: /etc/sentry-options/values/sentry-options-testing
          readOnly: true
      volumes:
      - name: options-values
        configMap:
          name: sentry-options-sentry-options-testing-default
```

Apply the deployment:

```bash
kubectl apply -f deployment.yaml
```

## 4. Verify Hot Reload

Watch the pod logs:

```bash
kubectl logs -f deployment/sentry-options-example
```

You should see output like:

```
values: UPDATED-VALUE | 0.99 | False
values: UPDATED-VALUE | 0.99 | False
```

Update a value and re-apply:

```bash
# Edit examples/configs/sentry-options-testing/default/values.yaml
# Change example-option to a new value

# Re-generate and apply ConfigMap
./target/release/sentry-options-cli write \
  --schemas examples/schemas \
  --root examples/configs \
  --output-format configmap \
  --namespace sentry-options-testing \
  --target default \
  | kubectl apply -f -
```

The pod will pick up the new values within ~5 seconds without restarting.

## Directory Structure

```
/etc/sentry-options/           # SENTRY_OPTIONS_DIR
├── schemas/                   # Baked into image
│   └── sentry-options-testing/
│       └── schema.json
└── values/                    # Mounted from ConfigMap
    └── sentry-options-testing/
        └── values.json
```
