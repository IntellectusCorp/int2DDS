<#
.SYNOPSIS
    Clean-build libint2dds_ffi.so for all Linux targets on Ubuntu 22.04.

.DESCRIPTION
    Produces forward-compatible (glibc 2.35) Linux shared libraries for every
    architecture the RMW layer needs, in one run:

        x86_64   -> ffi/dist/linux-x86_64/libint2dds_ffi.so
        arm64    -> ffi/dist/linux-aarch64/libint2dds_ffi.so
        armhf    -> ffi/dist/linux-armhf/libint2dds_ffi.so   (32-bit ARM)

    Built on Ubuntu 22.04 (glibc 2.35) -> runs on 22.04 AND 24.04.

    Each architecture is compiled NATIVELY inside its own-arch container
    (amd64 native; arm64/armhf via Docker Desktop's QEMU emulation), so the
    aws-lc-sys / ring crypto crates build the same way they would on real
    hardware. This is slower than cross-linking but reliable.

    Every run is a CLEAN build: ffi/dist is wiped first and the in-container
    Cargo target dir is ephemeral, so nothing is cached between runs.

    After building, a distributable archive is assembled (unless -NoPackage):

        ffi/dist/int2dds-ffi-<version>-linux.tar.gz
        ├── int2dds-ffi.manifest.yaml   # version/commit/per-arch sha256/min_glibc/soname
        ├── int2dds-ffi.h
        ├── LICENSE                      # repo-root Apache-2.0
        └── linux-x86_64/               # (one dir per built arch)
            ├── libint2dds_ffi.so         -> libint2dds_ffi.so.<major>  (dev symlink)
            ├── libint2dds_ffi.so.<major> -> libint2dds_ffi.so.<ver>    (soname symlink)
            └── libint2dds_ffi.so.<ver>                                 (real file)

    Each arch keeps its real .so plus the soname/dev symlinks, preserved as
    links in the archive (GNU tar + cp -d).

    sha256 and min_glibc are detected from the built binaries; version comes
    from [workspace.package].version in the root Cargo.toml.

.NOTES
    arm64/armhf builds run under emulation and can take many minutes each
    (aws-lc-rs + ring compile slowly under QEMU). This is expected.

.EXAMPLE
    .\ffi\docker\build-ffi-linux.ps1

.EXAMPLE
    .\ffi\docker\build-ffi-linux.ps1 -Only linux/arm64

.EXAMPLE
    .\ffi\docker\build-ffi-linux.ps1 -NoPackage   # build only, skip the tarball
#>
[CmdletBinding()]
param(
    [string]$RustVersion = "1.89.0",
    # Restrict to a subset, e.g. -Only linux/amd64,linux/arm64
    [string[]]$Only,
    # Skip assembling the .tar.gz distribution archive.
    [switch]$NoPackage
)

$ErrorActionPreference = "Stop"

# Run a native command (docker) with non-terminating error handling. BuildKit
# writes progress to stderr; under PowerShell 5.1 a merged-stream redirect
# (e.g. `script.ps1 *>&1 | Tee`) would otherwise wrap each stderr line as a
# terminating error. We gate success on the exit code instead.
function Invoke-Native {
    param([Parameter(Mandatory)][scriptblock]$Cmd, [string]$What = "command")
    $old = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { & $Cmd } finally { $ErrorActionPreference = $old }
    if ($LASTEXITCODE -ne 0) { throw "$What failed (exit $LASTEXITCODE)" }
}

# target triple-ish platform -> (docker platform, dist subdir)
# Musl targets build from the Alpine Dockerfile.musl (native musl); gnu targets
# from the Ubuntu Dockerfile. Each is compiled in its own-arch container.
$Targets = @(
    @{ Platform = "linux/amd64";  Dist = "linux-x86_64";       Musl = $false },
    @{ Platform = "linux/arm64";  Dist = "linux-aarch64";      Musl = $false },
    @{ Platform = "linux/arm/v7"; Dist = "linux-armhf";        Musl = $false },
    @{ Platform = "linux/amd64";  Dist = "linux-x86_64-musl";  Musl = $true  },
    @{ Platform = "linux/arm64";  Dist = "linux-aarch64-musl"; Musl = $true  }
)
if ($Only) { $Targets = $Targets | Where-Object { $Only -contains $_.Platform } }
if (-not $Targets) { throw "No targets selected (check -Only values)." }

$RepoRoot   = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$Dockerfile = Join-Path $PSScriptRoot "Dockerfile"
$DistRoot   = Join-Path $RepoRoot "ffi\dist"

Write-Host "Repo root : $RepoRoot"
Write-Host "Targets   : $($Targets.Platform -join ', ')"

# Docker engine reachable?
docker info --format '{{.ServerVersion}}' 2>$null | Out-Null
if (-not $?) { throw "Docker engine not reachable. Start Docker Desktop and retry." }

# Ensure up-to-date QEMU emulators are registered. Docker Desktop's bundled
# qemu can segfault (exit 139) inside libc-bin/ldconfig during arm64/armhf apt
# installs; refreshing binfmt with tonistiigi/binfmt fixes it. Registration is
# not persistent across Docker Desktop restarts, so we (re)apply it each run.
$needsEmulation = $Targets | Where-Object { $_.Platform -ne "linux/amd64" }
if ($needsEmulation) {
    Write-Host "`nRefreshing QEMU binfmt emulators (arm64, arm) ..." -ForegroundColor Yellow
    Invoke-Native -What "binfmt install" -Cmd {
        docker run --privileged --rm tonistiigi/binfmt:latest --install arm64,arm
    }
}

# Always clean: wipe previous artifacts.
if (Test-Path $DistRoot) {
    Write-Host "`nCleaning $DistRoot ..." -ForegroundColor Yellow
    Remove-Item -Recurse -Force $DistRoot
}
New-Item -ItemType Directory -Force -Path $DistRoot | Out-Null

$results = @()
$i = 0
foreach ($t in $Targets) {
    $i++
    $plat = $t.Platform
    $dist = $t.Dist
    $tag  = "int2dds-ffi-builder:$dist"

    Write-Host "`n===== [$i/$($Targets.Count)] $plat -> ffi/dist/$dist =====" -ForegroundColor Cyan

    # Pick the Dockerfile: Alpine (musl) vs Ubuntu (gnu).
    $df = if ($t.Musl) { Join-Path $PSScriptRoot "Dockerfile.musl" } else { $Dockerfile }

    # Build the per-platform toolchain image (layers cached after first run).
    Write-Host "[build image] $tag  (from $(Split-Path $df -Leaf))" -ForegroundColor DarkCyan
    Invoke-Native -What "docker build ($plat)" -Cmd {
        docker build --platform $plat `
            --build-arg "RUST_VERSION=$RustVersion" `
            -t $tag -f $df $PSScriptRoot
    }

    # Clean compile inside the container (ephemeral target dir).
    Write-Host "[compile] $plat (this may take a while under emulation)" -ForegroundColor DarkCyan
    Invoke-Native -What "container build ($plat / $dist)" -Cmd {
        docker run --rm --platform $plat `
            -v "${RepoRoot}:/src" `
            -e "DIST=$dist" `
            $tag
    }

    # The container now emits a versioned layout: libint2dds_ffi.so.<ver> (real),
    # libint2dds_ffi.so.<major> and libint2dds_ffi.so (soname/dev symlinks).
    $soLink = Join-Path $DistRoot "$dist\libint2dds_ffi.so"
    if (-not (Test-Path $soLink)) { throw "Artifact missing for $plat at $soLink" }
    # Real file is the largest libint2dds_ffi.so.* (the symlinks are tiny).
    $real = Get-ChildItem (Join-Path $DistRoot $dist) -Filter 'libint2dds_ffi.so.*' |
            Sort-Object Length -Descending | Select-Object -First 1
    $results += [pscustomobject]@{
        Platform = $plat
        Output   = "ffi/dist/$dist/$($real.Name)"
        SizeMB   = if ($real) { [math]::Round($real.Length / 1MB, 2) } else { 0 }
    }
}

Write-Host "`n===== DONE — all targets built =====" -ForegroundColor Green
$results | Format-Table -AutoSize
Write-Host "Header: ffi/dist/int2dds-ffi.h (architecture-independent)"

# --- Package: assemble int2dds-ffi-<ver>-linux.tar.gz -----------------------
# Done inside a Linux container so file modes, sha256sum, readelf and GNU tar
# all behave consistently (readelf reads any arch's ELF; tar preserves 0755).
if (-not $NoPackage) {
    Write-Host "`n===== Packaging distribution archive =====" -ForegroundColor Cyan

    $commit = (& git -C $RepoRoot rev-parse --short HEAD).Trim()
    $date   = Get-Date -Format 'yyyy-MM-dd'
    # Any builder image carries binutils/tar/coreutils; prefer the amd64 one.
    $pkgDist  = if ($Targets.Dist -contains 'linux-x86_64') { 'linux-x86_64' } else { $Targets[0].Dist }
    $pkgImage = "int2dds-ffi-builder:$pkgDist"

    # Bash payload. Single-quoted here-string => PowerShell does NOT expand $vars;
    # bash reads the runtime values from $GIT_COMMIT / $BUILD_DATE (passed via -e).
    $packageScript = @'
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
  soname=$(readelf -d "${stage}/${d}/libint2dds_ffi.so.${ver}" 2>/dev/null | grep -oP 'SONAME.*\[\K[^]]+')
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
'@

    # Write the script to a bind-mounted temp file and run `bash <file>`, rather
    # than `bash -c <arg>` or a stdin pipe: PowerShell 5.1 prepends a UTF-8 BOM
    # and rewrites line endings to CRLF when sending strings to a native command,
    # which breaks bash. Writing the file ourselves as UTF-8 (no BOM) with LF
    # endings sidesteps all of that.
    $pkgScriptPath = Join-Path $RepoRoot ".pkg-linux.sh"
    $lf = ($packageScript -replace "`r`n", "`n") -replace "`r", "`n"
    [System.IO.File]::WriteAllText($pkgScriptPath, $lf, (New-Object System.Text.UTF8Encoding($false)))
    try {
        Invoke-Native -What "package" -Cmd {
            docker run --rm `
                -v "${RepoRoot}:/src" `
                -e "GIT_COMMIT=$commit" `
                -e "BUILD_DATE=$date" `
                $pkgImage bash /src/.pkg-linux.sh
        }
    } finally {
        Remove-Item $pkgScriptPath -Force -ErrorAction SilentlyContinue
    }

    Write-Host "Archive : ffi/dist/int2dds-ffi-<version>-linux.tar.gz" -ForegroundColor Green
}
