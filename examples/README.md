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

## 1. Build and Push Docker Image

```bash
# From the repo root
docker build -t sentry-options-example -f examples/Dockerfile .

# Tag for Artifact Registry (replace REGION, PROJECT_ID, REPO)
docker tag sentry-options-example \
  REGION-docker.pkg.dev/PROJECT_ID/REPO/sentry-options-test:latest

# Push to registry
docker push REGION-docker.pkg.dev/PROJECT_ID/REPO/sentry-options-test:latest
```

Example for sandbox `sbx-hdeng-1`:

```bash
docker tag sentry-options-example \
  us-west1-docker.pkg.dev/eng-dev-sbx--hdeng-2/hdeng-images/sentry-options-test:latest
docker push us-west1-docker.pkg.dev/eng-dev-sbx--hdeng-2/hdeng-images/sentry-options-test:latest
```

## 2. Generate and Apply ConfigMap

```bash
# Generate ConfigMap and apply directly to cluster
./target/release/sentry-options-cli write \
  --schemas sentry-options/schemas \
  --root examples/configs \
  --output-format configmap \
  --namespace sentry-options-testing \
  --target default \
  | kubectl apply -f -
```

Or save to a file first:

```bash
./target/release/sentry-options-cli write \
  --schemas sentry-options/schemas \
  --root examples/configs \
  --output-format configmap \
  --namespace sentry-options-testing \
  --target default \
  --out configmap.yaml

kubectl apply -f configmap.yaml
```

## 3. Deploy Example Pod

An example deployment exists in `terraform-sandboxes.private`:

```
terraform-sandboxes.private/personal-sandbox/env/sbx-hdeng-1/k8s/deployment.yaml
```

The deployment mounts the ConfigMap as a volume:

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: sentry-options-test
spec:
  replicas: 1
  selector:
    matchLabels:
      app: sentry-options-test
  template:
    metadata:
      labels:
        app: sentry-options-test
    spec:
      containers:
      - name: test
        image: us-west1-docker.pkg.dev/eng-dev-sbx--hdeng-2/hdeng-images/sentry-options-test:latest
        volumeMounts:
        - name: values
          mountPath: /etc/sentry-options/values/sentry-options-testing
          readOnly: true
      volumes:
      - name: values
        configMap:
          name: sentry-options-sentry-options-testing
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
  --schemas sentry-options/schemas \
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
