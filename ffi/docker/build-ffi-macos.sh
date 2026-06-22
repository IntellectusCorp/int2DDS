#!/usr/bin/env bash
# Build libint2dds_ffi.dylib for macOS (x86_64 + arm64 + universal2) and package
# a .tar.gz. Run this ON a Mac. The install_name and compat/current versions are
# embedded by ffi/build.rs; min macOS is set via MACOSX_DEPLOYMENT_TARGET.
#
#   ffi/dist/int2dds-ffi-<version>-macos.tar.gz
#   ├── int2dds-ffi.manifest.yaml
#   ├── int2dds-ffi.h
#   ├── LICENSE
#   ├── macos-x86_64/libint2dds_ffi.dylib
#   ├── macos-arm64/libint2dds_ffi.dylib
#   └── macos-universal2/libint2dds_ffi.dylib
set -euo pipefail

export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-11.0}"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DIST="$REPO_ROOT/ffi/dist"
HEADER="$REPO_ROOT/ffi/include/int2dds-ffi.h"

ver=$(grep -m1 '^version' "$REPO_ROOT/Cargo.toml" | cut -d'"' -f2)
commit=$(git -C "$REPO_ROOT" rev-parse --short HEAD)
date=$(date +%Y-%m-%d)
echo "Version: $ver  Commit: $commit  Deployment target: $MACOSX_DEPLOYMENT_TARGET"

# Clean only macos-* dist subdirs.
rm -rf "$DIST"/macos-x86_64 "$DIST"/macos-arm64 "$DIST"/macos-universal2
mkdir -p "$DIST"

build_slice() {  # $1=triple  $2=distdir
  local triple="$1" dist="$2"
  echo "===== $triple -> ffi/dist/$dist ====="
  rustup target add "$triple" >/dev/null
  cargo build --release --target "$triple" -p int2dds-ffi
  mkdir -p "$DIST/$dist"
  cp "$REPO_ROOT/target/$triple/release/libint2dds_ffi.dylib" "$DIST/$dist/libint2dds_ffi.dylib"
}

build_slice x86_64-apple-darwin  macos-x86_64
build_slice aarch64-apple-darwin macos-arm64

# universal2 = lipo of both slices.
echo "===== universal2 (lipo) ====="
mkdir -p "$DIST/macos-universal2"
lipo -create \
  "$DIST/macos-x86_64/libint2dds_ffi.dylib" \
  "$DIST/macos-arm64/libint2dds_ffi.dylib" \
  -output "$DIST/macos-universal2/libint2dds_ffi.dylib"

# --- manifest + package ----------------------------------------------------
stage="$(mktemp -d)/int2dds-ffi-${ver}-macos"
mkdir -p "$stage"
cp "$HEADER" "$stage/int2dds-ffi.h"
cp "$REPO_ROOT/LICENSE" "$stage/LICENSE"
manifest="$stage/int2dds-ffi.manifest.yaml"
{
  echo "name: int2dds-ffi"
  echo "version: ${ver}"
  echo "git_commit: ${commit}"
  echo "build_date: \"${date}\""
  echo "api_header: int2dds-ffi.h"
  echo "license: Apache-2.0"
  echo "artifacts:"
} > "$manifest"

emit() {  # $1=arch  $2=triple  $3=distdir  [$4=slices]
  local arch="$1" triple="$2" dist="$3" slices="${4:-}"
  local f="$DIST/$dist/libint2dds_ffi.dylib"
  mkdir -p "$stage/$dist"
  cp "$f" "$stage/$dist/libint2dds_ffi.dylib"
  local sha; sha=$(shasum -a 256 "$f" | cut -d' ' -f1)
  {
    echo "  - os: macos"
    echo "    arch: ${arch}"
    echo "    triple: ${triple}"
    echo "    file: ${dist}/libint2dds_ffi.dylib"
    echo "    sha256: ${sha}"
    echo "    min_macos: \"${MACOSX_DEPLOYMENT_TARGET}\""
    [ -n "$slices" ] && echo "    slices: [${slices}]"
  } >> "$manifest"
}

emit amd64      x86_64-apple-darwin       macos-x86_64
emit arm64      aarch64-apple-darwin      macos-arm64
emit universal2 universal2-apple-darwin   macos-universal2 "x86_64, arm64"

out="$DIST/int2dds-ffi-${ver}-macos.tar.gz"
tar -czf "$out" -C "$stage" .

echo "== manifest =="; cat "$manifest"
echo "== install_name / versions (arm64) =="
otool -D "$DIST/macos-arm64/libint2dds_ffi.dylib"
otool -l "$DIST/macos-arm64/libint2dds_ffi.dylib" | grep -A3 LC_ID_DYLIB
echo "== universal2 slices =="; lipo -info "$DIST/macos-universal2/libint2dds_ffi.dylib"
echo "== archive =="; tar -tzf "$out"; ls -l "$out"
