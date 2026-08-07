#!/bin/bash
# Generate the ConfigMap for the example namespace/target and apply it to the
# current kubectl context. Re-run after editing examples/configs to push new
# values; the pod hot-reloads them within ~5s without a restart.
#
# Usage: examples/apply-configmap.sh [namespace] [target]
set -euo pipefail

NAMESPACE="${1:-sentry-options-testing}"
TARGET="${2:-default}"

# Run from repo root regardless of where the script is invoked.
cd "$(dirname "${BASH_SOURCE[0]}")/.."

CLI="target/release/sentry-options-cli"
if [[ ! -x "$CLI" ]]; then
    echo "Building sentry-options-cli..."
    cargo build --release -p sentry-options-cli
fi

"$CLI" write \
    --schemas sentry-options/schemas \
    --root examples/configs \
    --output-format configmap \
    --namespace "$NAMESPACE" \
    --target "$TARGET" \
    | kubectl apply -f -
