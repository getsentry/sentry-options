#!/bin/bash
set -euo pipefail

NEW_VERSION=$2

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# update python client library version
perl -i -pe "s/^version = \".*\"/version = \"$NEW_VERSION\"/" clients/python/pyproject.toml

# maturin build for python client
cd clients/python
maturin build --release --out ../../dist/ #root/dist
cd ../..
