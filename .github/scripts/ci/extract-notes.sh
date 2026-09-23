#!/usr/bin/env bash
# Extract the section for a given version from CHANGELOG.md.
#
# Usage: .github/scripts/ci/extract-notes.sh 0.1.3
# Reads CHANGELOG.md relative to the working directory, so it has to be run from
# the repo root.
set -euo pipefail

version="${1:?usage: extract-notes.sh <version>}"
base_version="${version%%-*}"

if [[ ! -f CHANGELOG.md ]]; then
  echo "ERROR: no CHANGELOG.md section found for version $base_version" >&2
  exit 1
fi

notes="$(awk -v ver="$base_version" '
  # Find a header of the form "## [0.1.3] - 2026-07-29" or "## [0.1.3] - TBD".
  $0 ~ "^## \\[" ver "\\]" { capture = 1; next }
  # Stop at the next "## [" header.
  capture && /^## \[/ { exit }
  capture { print }
' CHANGELOG.md)"

if [[ -z "$(echo "$notes" | tr -d '[:space:]')" ]]; then
  echo "ERROR: no CHANGELOG.md section found for version $base_version" >&2
  exit 1
fi

echo "$notes"
