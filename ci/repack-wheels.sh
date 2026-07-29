#!/usr/bin/env bash
# Inject the native library into the pure Python wheel and give it a platform tag.
#
# Usage: ci/repack-wheels.sh <version> <native_root>
#   Assumes native_root holds one directory per dist name, each containing the
#   extracted archive (lib/, bin/ ...).
set -euo pipefail

version="${1:?usage: repack-wheels.sh <version> <native_root>}"
native_root="${2:?}"

# Archive paths under native_root are named with the full tag version (prerelease
# suffix included, e.g. 0.2.0-rc.1), because ci/stage-native.sh takes that value
# straight into the filename. The wheel/sdist filenames and the directory name
# inside the wheel, on the other hand, come from the static version field in
# python/pyproject.toml (the base version without a suffix, enforced by
# ci/check-version.sh). Both forms are therefore used below: $version (full) to
# look up native paths, $base_version (suffix stripped) for wheel filenames and
# the internal directory name.
base_version="${version%%-*}"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out_dir="$repo_root/dist/wheels"
work="$repo_root/dist/wheel-work"

rm -rf "$out_dir" "$work"
mkdir -p "$out_dir" "$work"

# 1) Build the pure wheel + sdist once. These become the fallback assets.
python -m pip install --quiet --upgrade build wheel
python -m build --outdir "$out_dir" "$repo_root/python"

# The filename is deterministic — it comes from name/version (= base_version) in
# pyproject.toml — so no glob is needed. Probing with ls would die without a
# diagnostic under pipefail when nothing matches, so build the path directly and
# check it explicitly.
pure_wheel="$out_dir/int2dds-$base_version-py3-none-any.whl"
if [[ ! -f "$pure_wheel" ]]; then
  echo "ERROR: expected pure wheel not found at $pure_wheel (python -m build produced a different filename?)" >&2
  exit 1
fi

# 2) "dist name | library path inside the archive | wheel platform tag"
#    Only 6 are built; the fallback wheel covers the remaining platforms.
entries=(
  "windows-x86_64|bin/int2dds_ffi.dll|win_amd64"
  "windows-aarch64|bin/int2dds_ffi.dll|win_arm64"
  "linux-x86_64|lib/libint2dds_ffi.so.$version|manylinux_2_35_x86_64"
  "linux-aarch64|lib/libint2dds_ffi.so.$version|manylinux_2_35_aarch64"
  "macos-arm64|lib/libint2dds_ffi.dylib|macosx_11_0_arm64"
  "macos-x86_64|lib/libint2dds_ffi.dylib|macosx_10_12_x86_64"
)

for entry in "${entries[@]}"; do
  IFS='|' read -r dist rel tag <<< "$entry"
  src="$native_root/$dist/int2dds-$version-$dist/$rel"
  if [[ ! -f "$src" ]]; then
    echo "ERROR: native library not found for $dist at $src" >&2
    exit 1
  fi

  # When bundling, restore the standard filename the loader looks for
  # (the version-suffixed .so.0.1.1 -> libint2dds_ffi.so).
  case "$tag" in
    win_*)    dest_name="int2dds_ffi.dll" ;;
    macosx_*) dest_name="libint2dds_ffi.dylib" ;;
    *)        dest_name="libint2dds_ffi.so" ;;
  esac

  rm -rf "${work:?}/${tag:?}"
  mkdir -p "$work/$tag"
  ( cd "$work/$tag" && wheel unpack "$pure_wheel" >/dev/null )

  # The directory name wheel unpack creates follows the wheel's own metadata
  # version (= base_version), not the full tag version.
  unpacked="$work/$tag/int2dds-$base_version"
  mkdir -p "$unpacked/int2dds/_native"
  cp "$src" "$unpacked/int2dds/_native/$dest_name"

  # wheel pack --dest-dir does not create the target directory; it opens the file
  # inside it right away (as of wheel 0.47.0). Without mkdir first it raises
  # FileNotFoundError.
  mkdir -p "$work/$tag/out"
  ( cd "$work/$tag" && wheel pack "int2dds-$base_version" --dest-dir "$work/$tag/out" >/dev/null )
  # The output filename of wheel pack depends on normalization rules that vary by
  # wheel library version, so a real glob is needed here. On no match, ls fails to
  # find the literal "*.whl" (nullglob is unset), writes to stderr and would die
  # under errexit even without pipefail; absorb it with || true and check
  # explicitly so the script reports its own clear error instead.
  built="$(ls "$work/$tag/out"/*.whl 2>/dev/null || true)"
  if [[ ! -f "$built" ]]; then
    echo "ERROR: wheel pack produced no .whl in $work/$tag/out for tag $tag" >&2
    exit 1
  fi
  wheel tags --platform-tag "$tag" --remove "$built" >/dev/null
  mv "$work/$tag/out"/*.whl "$out_dir/"
done

echo "== wheels =="
ls -1 "$out_dir"
