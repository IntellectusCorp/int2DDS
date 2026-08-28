#!/usr/bin/env bash
# Clean-build libint2dds_ffi.so for every Linux target, then package a .tar.gz.
#
# Bash port of build-ffi-linux.ps1 — same targets, same output layout, same
# manifest. Runs on Linux, macOS, WSL and Git Bash; only Docker is required.
#
#   x86_64        -> ffi/dist/linux-x86_64/        (glibc 2.28, AlmaLinux 8)
#   arm64         -> ffi/dist/linux-aarch64/       (glibc 2.28, AlmaLinux 8)
#   armhf         -> ffi/dist/linux-armhf/         (glibc 2.35, Ubuntu 22.04, 32-bit ARM)
#   x86_64 musl   -> ffi/dist/linux-x86_64-musl/   (Alpine, no glibc)
#   arm64 musl    -> ffi/dist/linux-aarch64-musl/  (Alpine, no glibc)
#
# Each architecture is compiled NATIVELY inside its own-arch container: the
# host's own arch runs native, the rest run under QEMU emulation, so the
# ring crypto crate builds exactly as on real hardware.
#
# Every run is a CLEAN build: ffi/dist is wiped first and the in-container
# Cargo target dir is ephemeral, so nothing is cached between runs.
#
# Unless --no-package is given, the run ends with:
#   ffi/dist/int2dds-ffi-<version>-linux.tar.gz
#   ├── int2dds-ffi.manifest.yaml   # version/commit/per-arch sha256/min_glibc/soname
#   ├── int2dds-ffi.h
#   ├── LICENSE
#   └── linux-x86_64/               # one dir per built arch
#       ├── libint2dds_ffi.so         -> libint2dds_ffi.so.<major>  (dev symlink)
#       ├── libint2dds_ffi.so.<major> -> libint2dds_ffi.so.<ver>    (soname symlink)
#       └── libint2dds_ffi.so.<ver>                                 (real file)
#
# NOTE: emulated builds are slow (ring under QEMU). Expect ~3 min
# native, ~15 min arm64, ~23 min armhf.
set -euo pipefail

RUST_VERSION="1.89.0"
ONLY=()
NO_PACKAGE=0
NO_BINFMT=0
# QEMU is PINNED, not tracked to :latest -- see the binfmt section for why.
QEMU_VERSION="v8.1.5"

usage() {
  cat <<'USAGE'
Usage: build-ffi-linux.sh [options]

  --rust-version <x>   Toolchain to install in the builder image (default 1.89.0)
  --qemu-version <x>   tonistiigi/binfmt QEMU tag to register (default v8.1.5).
                       Do not raise this without re-testing an emulated gnu
                       target; 9.2.2 and 10.2.3 hang rustc (see the comments).
  --only <list>        Comma-separated subset. Accepts docker platforms
                       (linux/amd64, linux/arm64, linux/arm/v7) or dist dir
                       names (linux-x86_64, linux-aarch64-musl, ...).
                       A platform selects both its gnu and musl targets.
  --no-package         Build only; skip the .tar.gz distribution archive.
  --no-binfmt          Do not (re-)register QEMU emulators. Use when the
                       privileged binfmt container is unavailable and the
                       emulators are already known good.
  --refresh-binfmt     Accepted and ignored: refreshing is now the default.
  -h, --help           Show this help.

Examples:
  ./ffi/docker/build-ffi-linux.sh
  ./ffi/docker/build-ffi-linux.sh --only linux/arm64
  ./ffi/docker/build-ffi-linux.sh --only linux-x86_64,linux-x86_64-musl --no-package
USAGE
}

die() { echo "ERROR: $*" >&2; exit 1; }

split_csv() {  # $1 = "a,b,c" -> appends to ONLY
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
    --rust-version)   [ $# -ge 2 ] || die "--rust-version needs a value"; RUST_VERSION="$2"; shift 2 ;;
    --rust-version=*) RUST_VERSION="${1#*=}"; shift ;;
    --qemu-version)   [ $# -ge 2 ] || die "--qemu-version needs a value"; QEMU_VERSION="$2"; shift 2 ;;
    --qemu-version=*) QEMU_VERSION="${1#*=}"; shift ;;
    --only)           [ $# -ge 2 ] || die "--only needs a value"; split_csv "$2"; shift 2 ;;
    --only=*)         split_csv "${1#*=}"; shift ;;
    --no-package)     NO_PACKAGE=1; shift ;;
    --no-binfmt)      NO_BINFMT=1; shift ;;
    --refresh-binfmt) shift ;;   # kept for compatibility; refreshing is the default now
    -h|--help)        usage; exit 0 ;;
    *)                echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

# "<docker platform>|<dist subdir>|<libc>|<dockerfile>|<base image or ->".
# x86_64/aarch64 gnu build from manylinux_2_28 (AlmaLinux 8, glibc 2.28) so one
# artifact per arch covers RHEL 8/9/10 and Ubuntu 20.04+. manylinux has no armv7
# image, so armhf keeps its own Ubuntu 22.04 Dockerfile at floor 2.35. Musl
# targets build from the Alpine Dockerfile.musl (musl-native).
ALL_TARGETS="
linux/amd64|linux-x86_64|gnu|Dockerfile|quay.io/pypa/manylinux_2_28_x86_64
linux/arm64|linux-aarch64|gnu|Dockerfile|quay.io/pypa/manylinux_2_28_aarch64
linux/arm/v7|linux-armhf|gnu|Dockerfile.armhf|-
linux/amd64|linux-x86_64-musl|musl|Dockerfile.musl|-
linux/arm64|linux-aarch64-musl|musl|Dockerfile.musl|-
"

in_only() {  # $1 = candidate; true when ONLY is empty or contains it
  [ ${#ONLY[@]} -eq 0 ] && return 0
  local x
  for x in ${ONLY[@]+"${ONLY[@]}"}; do
    [ "$x" = "$1" ] && return 0
  done
  return 1
}

TARGETS=()
for entry in $ALL_TARGETS; do
  plat="${entry%%|*}"; rest="${entry#*|}"; dist="${rest%%|*}"
  if in_only "$plat" || in_only "$dist"; then
    TARGETS[${#TARGETS[@]}]="$entry"
  fi
done
[ ${#TARGETS[@]} -gt 0 ] || die "No targets selected (check --only values)."

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DIST_ROOT="$REPO_ROOT/ffi/dist"

# The container's own arch runs native; everything else needs QEMU.
case "$(uname -m)" in
  x86_64|amd64)   HOST_PLATFORM="linux/amd64"  ;;
  aarch64|arm64)  HOST_PLATFORM="linux/arm64"  ;;
  armv7l|armv7)   HOST_PLATFORM="linux/arm/v7" ;;
  *)              HOST_PLATFORM="unknown"      ;;
esac

echo "Repo root : $REPO_ROOT"
echo "Host      : $(uname -s) $(uname -m)  (native docker platform: $HOST_PLATFORM)"
printf 'Targets   :'
for entry in "${TARGETS[@]}"; do rest="${entry#*|}"; printf ' %s' "${rest%%|*}"; done
echo

# --- preflight --------------------------------------------------------------
command -v docker >/dev/null 2>&1 || die "docker not found in PATH. See the install notes in ffi/docker/README.md."
docker info --format '{{.ServerVersion}}' >/dev/null 2>&1 \
  || die "Docker engine not reachable. Start the Docker daemon (sudo systemctl start docker / Docker Desktop) and retry."

# Register a KNOWN-GOOD QEMU for every non-native platform. Two separate traps
# live here, and the fix for one is not the fix for the other:
#
#   1. Too OLD an emulator dies early and loudly -- a distro qemu 8.2.2 takes
#      "QEMU internal SIGSEGV" while rustup-init is still downloading, and
#      ldconfig/libc-bin segfault (exit 139) during emulated apt installs.
#
#   2. Too NEW an emulator dies late and silently. Measured on this repo,
#      x86_64 gnu `rustc --version` under qemu-user:
#          v7.0.0   OK (warns "MADV_DONTNEED does not work", jemalloc falls back)
#          v8.1.5   OK
#          v9.2.2   HANGS
#          v10.2.3  HANGS
#      rustc's bundled jemalloc probes MADV_DONTNEED. Old QEMU reports it
#      unsupported, so jemalloc takes its memset fallback and lives; newer QEMU
#      claims support it does not deliver, so jemalloc commits to a path that
#      then hangs or faults. musl targets are unaffected because the musl rustc
#      carries no jemalloc -- which is exactly why musl x86_64 built fine while
#      gnu x86_64 did not.
#
# So the version is PINNED rather than tracking :latest. Raising it requires
# re-running an emulated gnu target end to end, not just a smoke command.
# Registration is not persistent across daemon restarts, so re-apply each run.
BINFMT_IMAGE="tonistiigi/binfmt:qemu-$QEMU_VERSION"
if [ "$NO_BINFMT" -eq 0 ]; then
  binfmt_arches=""
  for entry in "${TARGETS[@]}"; do
    plat="${entry%%|*}"
    [ "$plat" = "$HOST_PLATFORM" ] && continue
    case "$plat" in
      linux/amd64)  a=amd64 ;;
      linux/arm64)  a=arm64 ;;
      linux/arm/v7) a=arm   ;;
      *)            a=""    ;;
    esac
    [ -n "$a" ] || continue
    case ",$binfmt_arches," in *",$a,"*) ;; *) binfmt_arches="${binfmt_arches:+$binfmt_arches,}$a" ;; esac
  done
  if [ -n "$binfmt_arches" ]; then
    echo
    echo "Registering QEMU binfmt emulators ($binfmt_arches) from $BINFMT_IMAGE ..."
    # ALWAYS uninstall before installing. tonistiigi/binfmt's --install SKIPS an
    # arch that is already registered -- it never replaces one. A Linux host with
    # binfmt-support/qemu-user-static already carries a registration from boot, so
    # a bare --install is a silent no-op and the build keeps using the distro's
    # emulator. When that one is too old it does not fail here; it fails minutes
    # later as an opaque apt GPG error or a QEMU internal SIGSEGV mid-compile.
    # Starting from a clean registration every run is the only way to make the
    # emulator a known quantity.
    #
    # This affects the WHOLE HOST, not just Docker. To hand the machine back to
    # the distro's own emulators afterwards:
    #     sudo systemctl restart systemd-binfmt   # or: sudo update-binfmts --enable
    # Pass --no-binfmt to skip this section entirely.
    echo "  clearing any existing registration first"
    docker run --privileged --rm "$BINFMT_IMAGE" --uninstall "$binfmt_arches" >/dev/null 2>&1 \
      || echo "  (nothing registered to clear)"
    binfmt_out="$(docker run --privileged --rm "$BINFMT_IMAGE" --install "$binfmt_arches" 2>&1)" \
      || { echo "$binfmt_out" >&2; die "binfmt registration failed. Re-run with --no-binfmt if the emulators are already known good."; }
    echo "$binfmt_out" | grep -E '^installing:' || true

    # Verify the kernel actually points at the container's emulators now. This is
    # cheap and catches the exact failure the uninstall above exists to prevent:
    # tonistiigi/binfmt installs to /usr/bin/qemu-*, while a distro registration
    # points into /usr/libexec/qemu-binfmt. Linux-only (no /proc elsewhere).
    if [ -d /proc/sys/fs/binfmt_misc ]; then
      for a in ${binfmt_arches//,/ }; do
        case "$a" in
          amd64) entry=qemu-x86_64  ;;
          arm64) entry=qemu-aarch64 ;;
          arm)   entry=qemu-arm     ;;
          *)     entry=""           ;;
        esac
        [ -n "$entry" ] && [ -r "/proc/sys/fs/binfmt_misc/$entry" ] || continue
        interp="$(sed -n 's/^interpreter //p' "/proc/sys/fs/binfmt_misc/$entry")"
        case "$interp" in
          /usr/bin/qemu-*) ;;
          *)
            echo "WARNING: $entry still resolves to $interp," >&2
            echo "  not the emulator tonistiigi/binfmt just installed. The build may die" >&2
            echo "  with a QEMU internal SIGSEGV. Clear it by hand and retry:" >&2
            echo "      docker run --privileged --rm $BINFMT_IMAGE --uninstall $a" >&2
            ;;
        esac
      done
    fi
  fi
fi

# --- helpers ----------------------------------------------------------------
# Containers run as root, so artifacts land root-owned on Linux bind mounts.
# Hand them back to the invoking user; otherwise the next run cannot wipe them.
fix_ownership() {  # $1 = image tag, $2 = docker platform
  [ "$(uname -s)" = "Linux" ] || return 0
  docker run --rm --platform "$2" -v "$DIST_ROOT:/d" "$1" \
    chown -R "$(id -u):$(id -g)" /d >/dev/null 2>&1 || true
}

file_size() {  # portable stat
  stat -c %s "$1" 2>/dev/null || stat -f %z "$1" 2>/dev/null || echo 0
}

# --- clean ------------------------------------------------------------------
if [ -d "$DIST_ROOT" ]; then
  echo
  echo "Cleaning $DIST_ROOT ..."
  rm -rf "$DIST_ROOT" 2>/dev/null || {
    echo "  host rm denied (root-owned leftovers) — removing inside a container"
    docker run --rm -v "$REPO_ROOT/ffi:/ffi" alpine:3.20 rm -rf /ffi/dist \
      || die "Could not remove $DIST_ROOT"
  }
fi
mkdir -p "$DIST_ROOT"

# --- build ------------------------------------------------------------------
RESULTS=()
i=0
for entry in "${TARGETS[@]}"; do
  i=$((i + 1))
  IFS='|' read -r plat dist libc dfname base <<< "$entry"
  tag="int2dds-ffi-builder:$dist"
  df="$SCRIPT_DIR/$dfname"

  # manylinux names its image per arch, so the gnu x86_64/aarch64 targets pass
  # BASE_IMAGE. The armhf and musl Dockerfiles pin their own FROM and take none.
  build_args=(--build-arg "RUST_VERSION=$RUST_VERSION")
  [ "$base" = "-" ] || build_args+=(--build-arg "BASE_IMAGE=$base")

  echo
  echo "===== [$i/${#TARGETS[@]}] $plat -> ffi/dist/$dist ====="

  # Build the per-platform toolchain image (layers cached after the first run).
  echo "[build image] $tag  (from $dfname)"
  docker build --platform "$plat" "${build_args[@]}" \
    -t "$tag" -f "$df" "$SCRIPT_DIR" \
    || die "docker build failed for $plat"

  # Clean compile inside the container (ephemeral target dir).
  echo "[compile] $plat (this may take a while under emulation)"
  docker run --rm --platform "$plat" \
    -v "$REPO_ROOT:/src" \
    -e "DIST=$dist" \
    "$tag" \
    || { fix_ownership "$tag" "$plat"; die "container build failed for $plat / $dist"; }

  fix_ownership "$tag" "$plat"

  # The container emits libint2dds_ffi.so.<ver> (real) plus the .so.<major> and
  # .so symlinks. Locate the one regular file among them.
  [ -e "$DIST_ROOT/$dist/libint2dds_ffi.so" ] \
    || die "Artifact missing for $plat at $DIST_ROOT/$dist/libint2dds_ffi.so"
  real=""
  for f in "$DIST_ROOT/$dist"/libint2dds_ffi.so.*; do
    [ -L "$f" ] && continue
    [ -f "$f" ] && real="$f"
  done
  [ -n "$real" ] || die "No real .so found under $DIST_ROOT/$dist"

  size_mb=$(awk -v b="$(file_size "$real")" 'BEGIN{printf "%.2f", b/1048576}')
  RESULTS[${#RESULTS[@]}]="$plat|ffi/dist/$dist/$(basename "$real")|$size_mb"
done

echo
echo "===== DONE — all targets built ====="
printf '%-14s %-52s %8s\n' "PLATFORM" "OUTPUT" "SIZE(MB)"
for r in "${RESULTS[@]}"; do
  p="${r%%|*}"; rest="${r#*|}"; o="${rest%%|*}"; s="${rest#*|}"
  printf '%-14s %-52s %8s\n' "$p" "$o" "$s"
done
echo "Header: ffi/dist/int2dds-ffi.h (architecture-independent)"

[ "$NO_PACKAGE" -eq 1 ] && exit 0

# --- package: assemble int2dds-ffi-<ver>-linux.tar.gz -----------------------
# Done inside a Linux container so file modes, sha256sum, readelf and GNU tar
# all behave consistently (readelf reads any arch's ELF; tar preserves 0755).
echo
echo "===== Packaging distribution archive ====="

commit="$(git -C "$REPO_ROOT" rev-parse --short HEAD)"
date_str="$(date +%Y-%m-%d)"

# Any builder image carries binutils/tar/coreutils. Score the built images and
# pick the best packaging host: the gnu images (AlmaLinux 8 for x86_64/arm64,
# Ubuntu 22.04 for armhf) carry full GNU coreutils where the Alpine ones only
# have busybox, and a natively-running image avoids pointlessly emulating the
# packaging step.
pkg_dist=""; pkg_plat=""; pkg_score=-1
for entry in "${TARGETS[@]}"; do
  IFS='|' read -r plat dist libc dfname base <<< "$entry"
  score=0
  [ "$libc" = "gnu" ] && score=$((score + 2))
  [ "$plat" = "$HOST_PLATFORM" ] && score=$((score + 1))
  if [ "$score" -gt "$pkg_score" ]; then
    pkg_score=$score; pkg_dist="$dist"; pkg_plat="$plat"
  fi
done
pkg_image="int2dds-ffi-builder:$pkg_dist"

pkg_script="$(mktemp "${TMPDIR:-/tmp}/int2dds-pkg-linux.XXXXXX.sh")"
trap 'rm -f "$pkg_script"' EXIT

cat > "$pkg_script" <<'PKG'
set -eu
ver=$(grep -m1 '^version' /src/Cargo.toml | cut -d'"' -f2)
stage="/tmp/int2dds-ffi-${ver}-linux"
rm -rf "$stage"; mkdir -p "$stage"

cp "/src/ffi/dist/int2dds-ffi.h" "$stage/"
cp "/src/LICENSE"                "$stage/LICENSE"

manifest="$stage/int2dds-ffi.manifest.yaml"
{
  echo "name: int2dds-ffi"
  echo "version: ${ver}"
  echo "git_commit: ${GIT_COMMIT}"
  echo "build_date: \"${BUILD_DATE}\""
  echo "api_header: int2dds-ffi.h"
  echo "license: Apache-2.0"
  echo "artifacts:"
} > "$manifest"

# "<dist-subdir>|<debian-arch>|<rust-triple>|<libc>"
for entry in \
  "linux-x86_64|amd64|x86_64-unknown-linux-gnu|gnu" \
  "linux-aarch64|arm64|aarch64-unknown-linux-gnu|gnu" \
  "linux-armhf|armhf|armv7-unknown-linux-gnueabihf|gnu" \
  "linux-x86_64-musl|amd64|x86_64-unknown-linux-musl|musl" \
  "linux-aarch64-musl|arm64|aarch64-unknown-linux-musl|musl"; do
  d=${entry%%|*}; rest=${entry#*|}; deb=${rest%%|*}; rest2=${rest#*|}; triple=${rest2%%|*}; libc=${rest2#*|}
  real="/src/ffi/dist/${d}/libint2dds_ffi.so.${ver}"
  if [ ! -e "$real" ]; then echo "  -- skip ${d}: not built this run"; continue; fi
  mkdir -p "${stage}/${d}"
  # Copy the real .so plus its soname/dev symlinks, preserving links as links
  # (-d = --no-dereference --preserve=links). This keeps the on-disk layout
  #   libint2dds_ffi.so -> .so.<major> -> .so.<ver>
  # intact inside the archive instead of flattening to a single file.
  cp -d "/src/ffi/dist/${d}/libint2dds_ffi.so"* "${stage}/${d}/"
  sha=$(sha256sum "${stage}/${d}/libint2dds_ffi.so.${ver}" | cut -d' ' -f1)
  # SONAME the runtime linker resolves (e.g. libint2dds_ffi.so.0), read from the ELF.
  # sed, not `grep -oP`: PCRE is GNU-only and the Alpine builder has busybox grep.
  soname=$(readelf -d "${stage}/${d}/libint2dds_ffi.so.${ver}" 2>/dev/null \
           | sed -n 's/.*SONAME.*\[\(.*\)\].*/\1/p' | head -1)
  [ -n "$soname" ] || soname="libint2dds_ffi.so.${ver%%.*}"
  {
    echo "  - os: linux"
    echo "    arch: ${deb}"
    echo "    triple: ${triple}"
    echo "    file: ${d}/libint2dds_ffi.so.${ver}"
    echo "    soname: ${soname}"
    echo "    sha256: ${sha}"
  } >> "$manifest"
  if [ "$libc" = musl ]; then
    echo "    libc: musl" >> "$manifest"
  else
    glibc=$(readelf -V "${stage}/${d}/libint2dds_ffi.so" 2>/dev/null | grep -oE 'GLIBC_[0-9.]+' | sed 's/GLIBC_//' | sort -V | tail -1)
    [ -n "$glibc" ] || glibc="unknown"
    echo "    min_glibc: \"${glibc}\"" >> "$manifest"
  fi
done

out="/src/ffi/dist/int2dds-ffi-${ver}-linux.tar.gz"
tar -czf "$out" -C "$stage" .
echo "== manifest =="; cat "$manifest"
echo "== archive contents =="; tar -tzf "$out"
echo "== archive =="; ls -l "$out"
PKG

docker run --rm --platform "$pkg_plat" \
  -v "$REPO_ROOT:/src" \
  -v "$pkg_script:/pkg.sh:ro" \
  -e "GIT_COMMIT=$commit" \
  -e "BUILD_DATE=$date_str" \
  "$pkg_image" bash /pkg.sh \
  || { fix_ownership "$pkg_image" "$pkg_plat"; die "packaging failed"; }

fix_ownership "$pkg_image" "$pkg_plat"

echo
echo "Archive : ffi/dist/int2dds-ffi-<version>-linux.tar.gz"
