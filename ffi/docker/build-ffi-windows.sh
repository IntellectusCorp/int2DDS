#!/usr/bin/env bash
# Build int2dds_ffi.dll for Windows targets and package a .zip.
#
# Bash port of build-ffi-windows.ps1 — same targets, same layout, same manifest.
# Unlike the Linux build this one does NOT use Docker: it drives cargo/rustup
# directly, so the toolchains have to exist on the machine.
#
#   ffi/dist/int2dds-ffi-<version>-windows.zip
#   ├── int2dds-ffi.manifest.yaml
#   ├── int2dds-ffi.h
#   ├── LICENSE
#   ├── windows-x86_64/int2dds_ffi.dll      (+ import lib)
#   ├── windows-i686/int2dds_ffi.dll
#   ├── windows-aarch64/int2dds_ffi.dll
#   └── windows-x86_64-gnu/int2dds_ffi.dll
#
# Where each target can be built:
#   *-pc-windows-msvc  -> on Windows only (Git Bash / MSYS2), needs the MSVC
#                         build tools; run from a Developer shell.
#   x86_64-pc-windows-gnu -> on Windows, and cross from Linux/macOS once the
#                         MinGW-w64 toolchain is installed:
#                           Ubuntu/Debian : sudo apt install mingw-w64
#                           Fedora        : sudo dnf install mingw64-gcc
#                           macOS         : brew install mingw-w64
#
# Targets whose toolchain is missing are SKIPPED with a message, never silently
# dropped — matching the PowerShell script's behaviour.
#
# The DLL filename is unversioned (Windows convention); the version lives in the
# embedded PE VERSIONINFO resource (see ffi/build.rs).
set -euo pipefail

ONLY=()
NO_PACKAGE=0

usage() {
  cat <<'USAGE'
Usage: build-ffi-windows.sh [options]

  --only <list>   Comma-separated rust triples, e.g.
                  --only x86_64-pc-windows-msvc,x86_64-pc-windows-gnu
  --no-package    Build only; skip the .zip distribution archive.
  -h, --help      Show this help.

Examples:
  ./ffi/docker/build-ffi-windows.sh
  ./ffi/docker/build-ffi-windows.sh --only x86_64-pc-windows-gnu --no-package
USAGE
}

die() { echo "ERROR: $*" >&2; exit 1; }

split_csv() {
  local IFS=','
  local item
  for item in $1; do
    [ -n "$item" ] && ONLY[${#ONLY[@]}]="$item"
  done
  # An empty trailing element leaves the AND-list at status 1; under `set -e`
  # that would abort at the call site, so normalise the function's status.
  return 0
}

while [ $# -gt 0 ]; do
  case "$1" in
    --only)       [ $# -ge 2 ] || die "--only needs a value"; split_csv "$2"; shift 2 ;;
    --only=*)     split_csv "${1#*=}"; shift ;;
    --no-package) NO_PACKAGE=1; shift ;;
    -h|--help)    usage; exit 0 ;;
    *)            echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

# "<triple>|<dist subdir>|<debian-ish arch>|<runtime>"
ALL_TARGETS="
x86_64-pc-windows-msvc|windows-x86_64|amd64|msvc
i686-pc-windows-msvc|windows-i686|x86|msvc
aarch64-pc-windows-msvc|windows-aarch64|arm64|msvc
x86_64-pc-windows-gnu|windows-x86_64-gnu|amd64|gnu
"

in_only() {
  [ ${#ONLY[@]} -eq 0 ] && return 0
  local x
  for x in ${ONLY[@]+"${ONLY[@]}"}; do
    [ "$x" = "$1" ] && return 0
  done
  return 1
}

TARGETS=()
for entry in $ALL_TARGETS; do
  in_only "${entry%%|*}" && TARGETS[${#TARGETS[@]}]="$entry"
done
[ ${#TARGETS[@]} -gt 0 ] || die "No targets selected (check --only values)."

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DIST_ROOT="$REPO_ROOT/ffi/dist"
HEADER="$REPO_ROOT/ffi/include/int2dds-ffi.h"

command -v cargo  >/dev/null 2>&1 || die "cargo not found in PATH."
command -v rustup >/dev/null 2>&1 || echo "WARN: rustup not found — assuming the targets are already installed."

echo "Repo root : $REPO_ROOT"
printf 'Targets   :'
for entry in "${TARGETS[@]}"; do printf ' %s' "${entry%%|*}"; done
echo

# Version = workspace SSOT (repo-root Cargo.toml [workspace.package]).
ver=$(grep -m1 '^version' "$REPO_ROOT/Cargo.toml" | cut -d'"' -f2)
[ -n "$ver" ] || die "Could not read version from root Cargo.toml"
echo "Version   : $ver"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1
  else shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

file_size() { stat -c %s "$1" 2>/dev/null || stat -f %z "$1" 2>/dev/null || echo 0; }

# PE VERSIONINFO is only readable with Windows tooling; elsewhere report "-".
pe_file_version() {
  if command -v powershell.exe >/dev/null 2>&1; then
    powershell.exe -NoProfile -Command \
      "(Get-Item -LiteralPath '$(cygpath -w "$1" 2>/dev/null || echo "$1")').VersionInfo.FileVersion" \
      2>/dev/null | tr -d '\r' | head -1 || true
  else
    echo "-"
  fi
  return 0
}

# Clean only the windows-* dist subdirs (leave linux/macos artifacts intact).
for entry in "${TARGETS[@]}"; do
  rest="${entry#*|}"; dist="${rest%%|*}"
  rm -rf "$DIST_ROOT/$dist"
done
mkdir -p "$DIST_ROOT"

RESULTS=()
for entry in "${TARGETS[@]}"; do
  triple="${entry%%|*}"; rest="${entry#*|}"
  dist="${rest%%|*}"; rest2="${rest#*|}"
  arch="${rest2%%|*}"; runtime="${rest2#*|}"

  echo
  echo "===== $triple -> ffi/dist/$dist ====="

  # Ensure the rustup target is installed (no-op if present).
  if command -v rustup >/dev/null 2>&1; then
    rustup target add "$triple" >/dev/null 2>&1 || {
      echo "[skip] $triple: rustup target add failed (target unavailable for this toolchain)"
      continue
    }
  fi

  # Build; a failure here means a missing cross toolchain -> skip this target.
  set +e
  cargo build --release --target "$triple" -p int2dds-ffi
  rc=$?
  set -e
  if [ $rc -ne 0 ]; then
    echo "[skip] $triple: build failed (toolchain missing?)"
    continue
  fi

  rel_dir="$REPO_ROOT/target/$triple/release"
  dll="$rel_dir/int2dds_ffi.dll"
  [ -f "$dll" ] || die "Expected DLL missing: $dll"

  out_dir="$DIST_ROOT/$dist"
  mkdir -p "$out_dir"
  cp "$dll" "$out_dir/int2dds_ffi.dll"

  # Import library: MSVC -> int2dds_ffi.dll.lib ; GNU -> libint2dds_ffi.dll.a
  for implib in int2dds_ffi.dll.lib libint2dds_ffi.dll.a; do
    [ -f "$rel_dir/$implib" ] && cp "$rel_dir/$implib" "$out_dir/$implib"
  done

  sha=$(sha256_of "$out_dir/int2dds_ffi.dll")
  fver=$(pe_file_version "$out_dir/int2dds_ffi.dll")
  [ -n "$fver" ] || fver="-"
  RESULTS[${#RESULTS[@]}]="$triple|$dist|$arch|$runtime|$sha|$fver"
done

[ ${#RESULTS[@]} -gt 0 ] || die "No Windows targets built successfully."

echo
echo "===== Built ====="
printf '%-26s %-14s %10s\n' "TRIPLE" "FILEVERSION" "SIZE(KB)"
for r in "${RESULTS[@]}"; do
  triple="${r%%|*}"; rest="${r#*|}"
  dist="${rest%%|*}"; rest2="${rest#*|}"
  rest3="${rest2#*|}"; rest4="${rest3#*|}"; fver="${rest4#*|}"
  kb=$(awk -v b="$(file_size "$DIST_ROOT/$dist/int2dds_ffi.dll")" 'BEGIN{printf "%.1f", b/1024}')
  printf '%-26s %-14s %10s\n' "$triple" "$fver" "$kb"
done

[ "$NO_PACKAGE" -eq 1 ] && exit 0

# --- package: int2dds-ffi-<ver>-windows.zip --------------------------------
echo
echo "===== Packaging archive ====="
commit="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
date_str="$(date +%Y-%m-%d)"

stage_root="$(mktemp -d "${TMPDIR:-/tmp}/int2dds-win.XXXXXX")"
trap 'rm -rf "$stage_root"' EXIT
stage="$stage_root/int2dds-ffi-$ver-windows"
mkdir -p "$stage"

cp "$HEADER" "$stage/int2dds-ffi.h"
cp "$REPO_ROOT/LICENSE" "$stage/LICENSE"

manifest="$stage/int2dds-ffi.manifest.yaml"
{
  echo "name: int2dds-ffi"
  echo "version: $ver"
  echo "git_commit: $commit"
  echo "build_date: \"$date_str\""
  echo "api_header: int2dds-ffi.h"
  echo "license: Apache-2.0"
  echo "artifacts:"
} > "$manifest"

for r in "${RESULTS[@]}"; do
  triple="${r%%|*}"; rest="${r#*|}"
  dist="${rest%%|*}"; rest2="${rest#*|}"
  arch="${rest2%%|*}"; rest3="${rest2#*|}"
  runtime="${rest3%%|*}"; rest4="${rest3#*|}"
  sha="${rest4%%|*}"
  mkdir -p "$stage/$dist"
  cp "$DIST_ROOT/$dist/int2dds_ffi.dll" "$stage/$dist/int2dds_ffi.dll"
  {
    echo "  - os: windows"
    echo "    arch: $arch"
    echo "    triple: $triple"
    echo "    file: $dist/int2dds_ffi.dll"
    echo "    sha256: $sha"
    echo "    runtime: $runtime"
    echo "    min_windows: \"10\""
  } >> "$manifest"
done

zip_out="$DIST_ROOT/int2dds-ffi-$ver-windows.zip"
rm -f "$zip_out"
if command -v zip >/dev/null 2>&1; then
  ( cd "$stage" && zip -qr "$zip_out" . )
elif command -v python3 >/dev/null 2>&1; then
  python3 - "$stage" "$zip_out" <<'PY'
import os, sys, zipfile
stage, out = sys.argv[1], sys.argv[2]
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for root, _, files in os.walk(stage):
        for f in files:
            p = os.path.join(root, f)
            z.write(p, os.path.relpath(p, stage))
PY
else
  die "Neither 'zip' nor 'python3' available to create the archive."
fi

echo
echo "== manifest =="; cat "$manifest"
echo
echo "Archive : ffi/dist/int2dds-ffi-$ver-windows.zip"
