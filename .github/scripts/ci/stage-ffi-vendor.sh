#!/usr/bin/env bash
# Assemble the tarball published to IntellectusCorp/int2dds_ffi_vendor releases.
#
# Usage: .github/scripts/ci/stage-ffi-vendor.sh <version> <artifact_root> <out_dir>
#
# This is NOT one of the 24 int2DDS release assets. The vendor repository is a
# separate release host consumed by the ROS 2 vendor package in
# IntellectusCorp/rmw_int2dds, and it wants one combined tarball rather than the
# per-architecture archives collect-assets.sh gathers:
#
#   int2dds-ffi-<version>-linux.tar.gz
#   ├── int2dds-ffi.h
#   ├── int2dds-ffi.manifest.yaml
#   ├── LICENSE
#   ├── linux-x86_64/       libint2dds_ffi.so -> .so.<major> -> .so.<version>
#   ├── linux-aarch64/
#   ├── linux-armhf/
#   ├── linux-x86_64-musl/
#   └── linux-aarch64-musl/
#
# Two rules the vendor package depends on, both asserted below rather than
# assumed:
#   1. `file:` must read exactly "<subdir>/libint2dds_ffi.so.<version>" -- that
#      string is the key it looks up to find the expected sha256. A mismatch
#      downgrades integrity verification to a warning instead of failing.
#   2. The SONAME symlink chain must survive. Without DT_SONAME the linker
#      records a file path in DT_NEEDED, which breaks Debian packaging.
#
# armhf-musl is deliberately absent: ROS binaries target glibc, and the vendor
# package ships armhf as gnu only.
set -euo pipefail

version="${1:?usage: stage-ffi-vendor.sh <version> <artifact_root> <out_dir>}"
artifact_root="${2:?}"
out_dir="${3:?}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

stage="$work/int2dds-ffi-${version}-linux"
mkdir -p "$stage" "$out_dir"

# "<dist subdir>|<debian arch>|<rust triple>"; order sets the manifest order.
targets="
linux-x86_64|amd64|x86_64-unknown-linux-gnu
linux-aarch64|arm64|aarch64-unknown-linux-gnu
linux-armhf|armhf|armv7-unknown-linux-gnueabihf
linux-x86_64-musl|amd64|x86_64-unknown-linux-musl
linux-aarch64-musl|arm64|aarch64-unknown-linux-musl
"

manifest="$stage/int2dds-ffi.manifest.yaml"
{
  echo "name: int2dds-ffi"
  echo "version: ${version}"
  echo "git_commit: $(git -C "$repo_root" rev-parse --short HEAD)"
  echo "build_date: \"$(date -u +%Y-%m-%d)\""
  echo "api_header: int2dds-ffi.h"
  echo "license: Apache-2.0"
  echo "artifacts:"
} > "$manifest"

for entry in $targets; do
  dist="${entry%%|*}"; rest="${entry#*|}"
  deb="${rest%%|*}"; triple="${rest#*|}"

  # Artifacts arrive as <artifact_root>/<artifact-name>/<file>, and the artifact
  # name is not the file name, so search by file name instead of guessing it.
  archive="$(find "$artifact_root" -type f -name "int2dds-${version}-${dist}.tar.gz" | head -1)"
  [ -n "$archive" ] || { echo "ERROR: no archive for ${dist} under ${artifact_root}" >&2; exit 1; }

  unpacked="$work/unpacked/$dist"
  mkdir -p "$unpacked"
  tar -xzf "$archive" -C "$unpacked"
  src="$unpacked/int2dds-${version}-${dist}"
  [ -d "$src" ] || { echo "ERROR: ${archive} does not contain int2dds-${version}-${dist}/" >&2; exit 1; }

  # -d keeps the symlinks as symlinks; dereferencing here would collapse the
  # SONAME chain into three identical 15 MB files and lose DT_SONAME resolution.
  mkdir -p "$stage/$dist"
  cp -d "$src/lib/libint2dds_ffi.so"* "$stage/$dist/"

  real="$stage/$dist/libint2dds_ffi.so.${version}"
  [ -f "$real" ] && [ ! -L "$real" ] \
    || { echo "ERROR: ${dist}: libint2dds_ffi.so.${version} missing or not a regular file" >&2; exit 1; }
  for link in "libint2dds_ffi.so" "libint2dds_ffi.so.${version%%.*}"; do
    [ -L "$stage/$dist/$link" ] \
      || { echo "ERROR: ${dist}: ${link} is not a symlink -- SONAME chain broken" >&2; exit 1; }
  done

  # Reuse the per-architecture manifest rather than recomputing: stage-native.sh
  # already derived these from the ELF and failed the build if min_glibc broke
  # the per-arch ceiling. Recomputing here would be a second, weaker opinion.
  src_manifest="$src/manifest.txt"
  [ -f "$src_manifest" ] || { echo "ERROR: ${dist}: manifest.txt missing" >&2; exit 1; }
  sha="$(sed -n 's/^sha256: //p'  "$src_manifest")"
  soname="$(sed -n 's/^soname: //p' "$src_manifest")"
  [ -n "$sha" ] && [ -n "$soname" ] \
    || { echo "ERROR: ${dist}: manifest.txt has no sha256/soname" >&2; exit 1; }

  {
    echo "  - os: linux"
    echo "    arch: ${deb}"
    echo "    triple: ${triple}"
    echo "    file: ${dist}/libint2dds_ffi.so.${version}"
    echo "    soname: ${soname}"
    echo "    sha256: ${sha}"
  } >> "$manifest"

  case "$dist" in
    *-musl) echo "    libc: musl" >> "$manifest" ;;
    *)
      glibc="$(sed -n 's/^min_glibc: "\(.*\)"$/\1/p' "$src_manifest")"
      [ -n "$glibc" ] || { echo "ERROR: ${dist}: manifest.txt has no min_glibc" >&2; exit 1; }
      echo "    min_glibc: \"${glibc}\"" >> "$manifest"
      ;;
  esac

  # The header is architecture-independent; take it from the first archive that
  # has one so the tarball does not depend on which arch built last.
  [ -f "$stage/int2dds-ffi.h" ] || cp "$src/include/int2dds-ffi.h" "$stage/"
  [ -f "$stage/LICENSE" ]       || cp "$src/LICENSE"               "$stage/"
done

for required in int2dds-ffi.h LICENSE int2dds-ffi.manifest.yaml; do
  [ -f "$stage/$required" ] || { echo "ERROR: ${required} missing from the staged tree" >&2; exit 1; }
done

# Files sit at the archive root, NOT under a version-named directory. The
# vendor package extracts straight into its install prefix and looks up
# "<subdir>/libint2dds_ffi.so.<version>" from there, so a wrapper directory
# would push every path one level down and break every manifest lookup.
archive="$out_dir/int2dds-ffi-${version}-linux.tar.gz"
tar -czf "$archive" -C "$stage" .

echo "== manifest =="; cat "$manifest"
echo "== archive contents =="; tar -tzvf "$archive"
echo "== archive =="; ls -l "$archive"
