#!/bin/bash
set -euxo pipefail

cd src

# by default points to the examples schemas/values
NAMESPACE="${NAMESPACE:-sentry-options-testing}"
TARGET="${TARGET:-default}"
SCHEMAS_DIR="${SCHEMAS_DIR:-sentry-options/schemas}"
CONFIGS_DIR="${CONFIGS_DIR:-examples/configs}"

COMMIT_SHA="${GO_REVISION_NEW_SENTRY_OPTIONS:-$(git rev-parse HEAD)}"
COMMIT_TIMESTAMP=$(git log -1 --pretty=%ct)

/devinfra/scripts/get-cluster-credentials

echo "Building sentry-options-cli..."
cargo build --release -p sentry-options-cli

# Generate and apply ConfigMap
echo "Generating ConfigMap for namespace=${NAMESPACE}, target=${TARGET}..."
./target/release/sentry-options-cli write \
  --schemas "${SCHEMAS_DIR}" \
  --root "${CONFIGS_DIR}" \
  --output-format configmap \
  --namespace "${NAMESPACE}" \
  --target "${TARGET}" \
  --commit-sha "${COMMIT_SHA}" \
  --commit-timestamp "${COMMIT_TIMESTAMP}" |
  kubectl apply --server-side --force-conflicts -f -

echo "ConfigMap sentry-options-${NAMESPACE}-${TARGET} applied successfully"
