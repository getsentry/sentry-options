#!/bin/sh
# Run prek on the files a change added or modified, so deleted files are skipped.
# CHANGED_FILES is a JSON array of paths, as written by dorny/paths-filter.
set -eu

files=$(jq --raw-output '.[]' <<JSON
$CHANGED_FILES
JSON
)

set --
while IFS= read -r file; do
    [ -n "$file" ] || continue
    set -- "$@" "$file"
done <<FILES
$files
FILES

if [ "$#" -eq 0 ]; then
    echo 'no added or modified files to check'
    exit 0
fi

exec prek run --files "$@"
