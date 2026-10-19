#!/usr/bin/env bash
# Generate the release SBOM (SPDX + CycloneDX) from the source tree and check
# the NTIA minimum elements.
#
# Usage: .github/scripts/ci/generate-sbom.sh <version> <out_dir>
# Needs cargo-cyclonedx, syft and jq on PATH. Run from the repo root.
set -euo pipefail

version="${1:?usage: generate-sbom.sh <version> <out_dir>}"
out_dir="${2:?}"
supplier="IntellectusCorp"
spdx="$out_dir/int2dds-$version.spdx.json"
cdx="$out_dir/int2dds-$version.cdx.json"

# Syft would otherwise pick up a previous run's output as an input SBOM.
rm -rf "$out_dir"

# Syft's Cargo.lock cataloger records no licenses, so the Rust graph comes from
# cargo-cyclonedx (one SBOM per crate, all target platforms, no dev- or
# build-dependencies) and Syft's sbom-cataloger merges those per-crate files.
tmp="$(mktemp -d)"
cargo cyclonedx --format json --spec-version 1.5 --target all --no-build-deps
# Cargo targets (lib/bin) would otherwise show up as duplicate, unlicensed crates.
find . -name '*.cdx.json' -not -path './target/*' | while read -r f; do
  jq 'del(.metadata.component.components)' "$f" > "$tmp/crate.json" && mv "$tmp/crate.json" "$f"
done

# Syft reads neither pyproject.toml nor .csproj, so the runtime dependencies of the
# Python and C# packages are described from their manifests. Python declares only
# version ranges, which get no version in the purl. Only the package's own license
# is known from its manifest; the dependencies get none.
# Usage: binding_sbom <out> <purl type> <name> <version> <license>,
# "name<TAB>version" on stdin.
binding_sbom() {
  jq -nR --arg type "$2" --arg name "$3" --arg version "$4" --arg license "$5" '
    def comp($n; $v): {type: "library", "bom-ref": "\($type):\($n)@\($v)", name: $n, version: $v,
      purl: (if $v | test("^[0-9]") then "pkg:\($type)/\($n)@\($v)" else "pkg:\($type)/\($n)" end)};
    [inputs | split("\t") | comp(.[0]; .[1])] as $deps
    | (comp($name; $version) + {licenses: [{expression: $license}]}) as $root
    | {bomFormat: "CycloneDX", specVersion: "1.5", version: 1,
       metadata: {component: $root}, components: $deps,
       dependencies: [{ref: $root["bom-ref"], dependsOn: [$deps[]["bom-ref"]]}]}' > "$1"
}
base_version="${version%%-*}"
sed -n '/^dependencies = \[/,/^\]/p' python/pyproject.toml | grep -oE '"[^"]+"' | tr -d '"' \
  | sed -E 's/;.*//; s/^([A-Za-z0-9._-]+) *(.*)$/\1\t\2/' \
  | binding_sbom python/int2dds-python.cdx.json pypi int2dds "$base_version" \
      "$(sed -nE 's/^license = \{text = "([^"]+)"\}/\1/p' python/pyproject.toml)"
# PrivateAssets="all" references are build-time only and never reach consumers.
grep '<PackageReference ' csharp/src/Int2Dds/Int2Dds.csproj | grep -v 'PrivateAssets="all"' \
  | sed -E 's/.*Include="([^"]+)".*Version="([^"]+)".*/\1\t\2/' | sort -u \
  | binding_sbom csharp/src/Int2Dds/int2dds-csharp.cdx.json nuget Int2Dds "$base_version" \
      "$(sed -nE 's/.*<PackageLicenseExpression>([^<]+)<.*/\1/p' csharp/src/Int2Dds/Int2Dds.csproj)"
SYFT_FILE_METADATA_SELECTION=none syft scan dir:. --quiet \
  --source-name int2dds --source-version "$version" \
  --select-catalogers "+sbom-cataloger,-rust-cargo-lock-cataloger,-github-actions-usage-cataloger,-github-action-workflow-usage-cataloger" \
  --exclude './java/gradle/**' \
  -o "spdx-json=$tmp/spdx.json" -o "cyclonedx-json@1.5=$tmp/cdx.json"
find . -name '*.cdx.json' -not -path './target/*' -delete
mkdir -p "$out_dir"

# --source-supplier would stamp every third-party package too; set it on the root
# only. Syft names Anchore as the author and leaves the root out of the CycloneDX
# graph, and still lists the per-crate input files in SPDX.
jq --arg s "$supplier" '
  .metadata.supplier = {name: $s} | .metadata.component.supplier = {name: $s}
  | .metadata.authors = [{name: $s}]
  | ([.dependencies[].dependsOn[]?] | unique) as $deps
  | .dependencies += [{ref: .metadata.component["bom-ref"],
      dependsOn: [.components[] | .["bom-ref"] | select(IN($deps[]) | not)]}]' \
  "$tmp/cdx.json" > "$cdx"
jq --arg s "Organization: $supplier" '
  [.relationships[] | select(.relationshipType == "DESCRIBES") | .relatedSpdxElement] as $roots
  | [.files[]?.SPDXID] as $files
  | .packages |= map(if .SPDXID | IN($roots[]) then .supplier = $s else . end)
  | .creationInfo.creators |= map(if startswith("Organization:") then $s else . end)
  | del(.files)
  | .relationships |= map(select((.relatedSpdxElement | IN($files[])) | not))' \
  "$tmp/spdx.json" > "$spdx"
rm -rf "$tmp"

# NTIA minimum elements: supplier, component name, version, unique identifier,
# dependency relationship, author, timestamp.
jq -e --arg v "$version" '
  .bomFormat == "CycloneDX"
  and .metadata.timestamp
  and (.metadata.tools.components | length > 0)
  and .metadata.supplier.name
  and (.metadata.authors | length > 0)
  and .metadata.component.name == "int2dds" and .metadata.component.version == $v
  and (.metadata.component["bom-ref"] as $r | [.dependencies[] | select(.ref == $r)] | length == 1)
  and ([.components[] | select(.type == "library")] | length > 0)
  and ([.components[] | select(.type == "library" and ((.name and .version and .purl) | not))] | length == 0)
' "$cdx" > /dev/null || { echo "ERROR: $cdx is missing NTIA minimum elements" >&2; exit 1; }
jq -e --arg s "Organization: $supplier" '
  (.spdxVersion | startswith("SPDX-"))
  and .creationInfo.created
  and (.creationInfo.creators | index($s))
  and (.packages | length > 1)
  and ([.relationships[] | select(.relationshipType == "DESCRIBES")] | length > 0)
  and ([.relationships[] | select(.relationshipType == "DEPENDENCY_OF" or .relationshipType == "DEPENDS_ON")] | length > 0)
' "$spdx" > /dev/null || { echo "ERROR: $spdx is not a valid SPDX document" >&2; exit 1; }

jq -r '
  [.components[] | select(.type == "library")] as $libs
  | "components: \($libs | length) (license declared: \([$libs[] | select(.licenses)] | length))",
    ($libs[] | select(.licenses | not) | "  no license: \(.purl)")
' "$cdx"
ls -1 "$out_dir"
