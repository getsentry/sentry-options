#!/bin/bash
set -euo pipefail

NEW_VERSION="${2}"

cd "$(dirname "${BASH_SOURCE[0]}")/.."

perl -i -pe "s/^version = \".*\"/version = \"$NEW_VERSION\"/" clients/python/pyproject.toml
perl -i -pe "s/^version = \".*\"/version = \"${NEW_VERSION}\"/" Cargo.toml
perl -i -pe "s/(sentry-options-validation = \{ path = \"[^\"]*\", version = \")[^\"]*/\${1}${NEW_VERSION}/" Cargo.toml

cargo metadata --format-version 1 > /dev/null
