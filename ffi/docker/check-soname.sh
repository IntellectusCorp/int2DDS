#!/usr/bin/env bash
# Verify the DT_SONAME embedded in the built libint2dds_ffi.so artifacts.
#
# Bash port of check-soname.ps1. Reads the ELF DT_SONAME dynamic tag from every
# libint2dds_ffi.so.<ver> produced by build-ffi-linux.sh under ffi/dist and
# prints the soname per architecture. Handles 64-bit (x86_64, aarch64) and
# 32-bit (armhf) objects.
#
# The soname is embedded by ffi/build.rs as libint2dds_ffi.so.<major>, derived
# from the workspace version in the repo-root Cargo.toml. For version 0.1.1 the
# expected soname is therefore "libint2dds_ffi.so.0".
#
# Uses readelf when binutils is installed and falls back to a pure-python ELF
# reader (macOS, minimal containers), so no Docker/WSL is required either way.
#
# Exits 1 when any artifact has a missing or unexpected SONAME.
set -euo pipefail

DIST_ROOT=""
SINGLE=""

usage() {
  cat <<'USAGE'
Usage: check-soname.sh [options]

  --dist-root <dir>  Directory to scan (default: ffi/dist next to this script)
  --path <file>      Check a single .so instead of scanning the dist tree
  -h, --help         Show this help.

Examples:
  ./ffi/docker/check-soname.sh
  ./ffi/docker/check-soname.sh --path ffi/dist/linux-x86_64/libint2dds_ffi.so.0.1.1
USAGE
}

die() { echo "ERROR: $*" >&2; exit 1; }

while [ $# -gt 0 ]; do
  case "$1" in
    --dist-root)   [ $# -ge 2 ] || die "--dist-root needs a value"; DIST_ROOT="$2"; shift 2 ;;
    --dist-root=*) DIST_ROOT="${1#*=}"; shift ;;
    --path)        [ $# -ge 2 ] || die "--path needs a value"; SINGLE="$2"; shift 2 ;;
    --path=*)      SINGLE="${1#*=}"; shift ;;
    -h|--help)     usage; exit 0 ;;
    *)             echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Print the DT_SONAME of an ELF shared object on stdout, or nothing when absent.
elf_soname() {
  local file="$1"
  if command -v readelf >/dev/null 2>&1; then
    readelf -d "$file" 2>/dev/null \
      | sed -n 's/.*SONAME.*\[\(.*\)\].*/\1/p' | head -1
    return 0
  fi
  # readelf-free fallback: walk the section headers to SHT_DYNAMIC, then the
  # dynamic entries to DT_SONAME (tag 14), resolving the string via .dynstr.
  python3 - "$file" <<'PY'
import struct, sys
b = open(sys.argv[1], "rb").read()
if len(b) < 64 or b[:4] != b"\x7fELF":
    sys.exit("Not an ELF file: " + sys.argv[1])
if b[5] != 1:
    sys.exit("Big-endian ELF not supported by this helper: " + sys.argv[1])
is64 = b[4] == 2
if is64:
    shoff, = struct.unpack_from("<Q", b, 0x28)
    shent, shnum = struct.unpack_from("<HH", b, 0x3A)
else:
    shoff, = struct.unpack_from("<I", b, 0x20)
    shent, shnum = struct.unpack_from("<HH", b, 0x2E)
dyn = None
for i in range(shnum):
    sh = shoff + i * shent
    if struct.unpack_from("<I", b, sh + 4)[0] == 6:  # SHT_DYNAMIC
        if is64:
            off, size = struct.unpack_from("<QQ", b, sh + 24)
            link, = struct.unpack_from("<I", b, sh + 40)
        else:
            off, size, link = struct.unpack_from("<III", b, sh + 16)
        dyn = (off, size, link)
        break
if dyn is None:
    sys.exit("No .dynamic section (not a shared object?): " + sys.argv[1])
off, size, link = dyn
shl = shoff + link * shent
stroff = struct.unpack_from("<Q", b, shl + 24)[0] if is64 else struct.unpack_from("<I", b, shl + 16)[0]
step = 16 if is64 else 8
fmt = "<QQ" if is64 else "<II"
for o in range(off, off + size, step):
    tag, val = struct.unpack_from(fmt, b, o)
    if tag == 0:
        break
    if tag == 14:  # DT_SONAME
        s = stroff + val
        print(b[s:b.index(b"\0", s)].decode("ascii"))
        break
PY
}

# --- single-file mode -------------------------------------------------------
if [ -n "$SINGLE" ]; then
  [ -e "$SINGLE" ] || die "No such file: $SINGLE"
  if ! soname="$(elf_soname "$SINGLE" 2>&1)"; then
    die "$soname"
  fi
  if [ -n "$soname" ]; then
    echo "SONAME: $soname  <- $SINGLE"
  else
    echo "NO SONAME embedded  <- $SINGLE"
    exit 1
  fi
  exit 0
fi

# --- scan mode --------------------------------------------------------------
[ -n "$DIST_ROOT" ] || DIST_ROOT="$SCRIPT_DIR/../dist"
[ -d "$DIST_ROOT" ] || die "Dist directory not found: $DIST_ROOT. Run build-ffi-linux.sh first."

# Real artifacts are libint2dds_ffi.so.<major>.<minor>.<patch> (the .so and
# .so.<major> entries are symlinks, and -type f skips them). The expected
# SONAME is the major-only form, e.g. libint2dds_ffi.so.0.
files=$(find "$DIST_ROOT" -type f -name 'libint2dds_ffi.so.*' | sort)

found=0
bad=0
printf '%-22s %-32s %s\n' "ARCH" "FILE" "SONAME"
while IFS= read -r f; do
  [ -n "$f" ] || continue
  base="$(basename "$f")"
  # Only the three-segment version form is a real artifact.
  [[ "$base" =~ ^libint2dds_ffi\.so\.[0-9]+\.[0-9]+\.[0-9]+$ ]] || continue
  found=$((found + 1))
  arch="$(basename "$(dirname "$f")")"
  if ! soname="$(elf_soname "$f" 2>&1)"; then
    soname="ERROR: $soname"
  fi
  [ -n "$soname" ] || soname="(none)"
  printf '%-22s %-32s %s\n' "$arch" "$base" "$soname"
  [[ "$soname" =~ ^libint2dds_ffi\.so\.[0-9]+$ ]] || bad=1
done <<EOF
$files
EOF

[ "$found" -gt 0 ] || die "No libint2dds_ffi.so.<ver> files under $DIST_ROOT. Run build-ffi-linux.sh first."

echo
if [ "$bad" -ne 0 ]; then
  echo "WARNING: one or more artifacts have a missing/unexpected SONAME." >&2
  exit 1
fi
echo "All artifacts carry a versioned SONAME."
