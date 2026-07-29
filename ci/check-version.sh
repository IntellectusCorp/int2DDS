#!/usr/bin/env bash
# Verify that the tag version matches every version declaration in the repo.
#
# Usage: ci/check-version.sh v0.1.1
# On success, prints the bare version ("0.1.1") to stdout and exits 0.
set -euo pipefail

tag="${1:?usage: check-version.sh <tag>}"
version="${tag#v}"

if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
  echo "ERROR: tag '$tag' is not a semver tag (expected vMAJOR.MINOR.PATCH[-PRERELEASE])" >&2
  exit 1
fi

# The prerelease suffix is stripped before comparing against the source versions.
base_version="${version%%-*}"

fail=0

# Cargo.toml — only the version key under [workspace.package].
# Anchored at line start so it does not pick up rust-version in the same file.
cargo_version="$(grep -m1 -E '^version[[:space:]]*=' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
if [[ "$cargo_version" != "$base_version" ]]; then
  echo "ERROR: Cargo.toml version '$cargo_version' != tag version '$base_version'" >&2
  fail=1
fi

# python/pyproject.toml — hardcoded, so it can drift away from Cargo.toml.
py_version="$(grep -m1 -E '^version[[:space:]]*=' python/pyproject.toml | sed -E 's/.*"([^"]+)".*/\1/')"
if [[ "$py_version" != "$base_version" ]]; then
  echo "ERROR: python/pyproject.toml version '$py_version' != tag version '$base_version'" >&2
  fail=1
fi

# csharp/Directory.Build.props reads Cargo.toml with a regex that takes the first
# match in the file, and rust-version matches the same pattern. Confirm that the
# first version match in Cargo.toml really is the workspace version.
first_match="$(grep -m1 -E 'version[[:space:]]*=[[:space:]]*"' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
if [[ "$first_match" != "$base_version" ]]; then
  echo "ERROR: the first 'version = \"...\"' match in Cargo.toml is '$first_match', not '$base_version'." >&2
  echo "       csharp/Directory.Build.props reads that first match, so the C# package version would be wrong." >&2
  fail=1
fi

if [[ "$fail" -ne 0 ]]; then
  exit 1
fi

echo "$version"
