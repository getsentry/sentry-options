#!/bin/sh
# Craft runs this as the preReleaseCommand with the old and new versions as $1 and $2.
set -eu

NEW_VERSION="${2}"
export NEW_VERSION

script_dir=$(dirname "$0")
cd "$script_dir/.."

perl -i -pe 's/^version = ".*"/version = "$ENV{NEW_VERSION}"/' clients/python/pyproject.toml
perl -i -pe 's/^version = ".*"/version = "$ENV{NEW_VERSION}"/' Cargo.toml
perl -i -pe 's/(sentry-options-validation = \{ path = "[^"]*", version = ")[^"]*/${1}$ENV{NEW_VERSION}/' Cargo.toml
perl -i -pe 'if (/cli-version:/ .. /default:/) { s/(default: ")[0-9.]+"/${1}$ENV{NEW_VERSION}"/ }' .github/workflows/validate-schema.yml

cargo metadata --format-version 1 > /dev/null
cd clients/python
uv lock
