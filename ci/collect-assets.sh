#!/usr/bin/env bash
# Gather the downloaded artifacts into one directory, generate SHA256SUMS and
# verify the count.
#
# Usage: ci/collect-assets.sh <version> <artifact_root> <out_dir>
set -euo pipefail

version="${1:?usage: collect-assets.sh <version> <artifact_root> <out_dir>}"
artifact_root="${2:?}"
out_dir="${3:?}"

rm -rf "$out_dir"
mkdir -p "$out_dir"

# Artifacts arrive as <artifact_root>/<artifact-name>/<file>.
find "$artifact_root" -type f \
  \( -name '*.tar.gz' -o -name '*.zip' -o -name '*.nupkg' -o -name '*.snupkg' -o -name '*.whl' \) \
  -exec cp {} "$out_dir/" \;

expected=24
actual="$(find "$out_dir" -maxdepth 1 -type f | wc -l | tr -d ' ')"
if [[ "$actual" -ne "$expected" ]]; then
  echo "ERROR: expected $expected release archives, found $actual" >&2
  ls -1 "$out_dir" >&2
  exit 1
fi

# Check by name that all 13 native archives are present.
for dist in linux-x86_64 linux-aarch64 linux-armhf \
            linux-x86_64-musl linux-aarch64-musl linux-armhf-musl \
            macos-x86_64 macos-arm64 macos-universal2; do
  if [[ ! -f "$out_dir/int2dds-$version-$dist.tar.gz" ]]; then
    echo "ERROR: missing archive int2dds-$version-$dist.tar.gz" >&2
    exit 1
  fi
done
for dist in windows-x86_64 windows-i686 windows-aarch64 windows-x86_64-gnu; do
  if [[ ! -f "$out_dir/int2dds-$version-$dist.zip" ]]; then
    echo "ERROR: missing archive int2dds-$version-$dist.zip" >&2
    exit 1
  fi
done

( cd "$out_dir" && sha256sum ./* > SHA256SUMS )

echo "== release assets =="
ls -1 "$out_dir"
echo "total: $(find "$out_dir" -maxdepth 1 -type f | wc -l | tr -d ' ') files (24 archives + SHA256SUMS)"
