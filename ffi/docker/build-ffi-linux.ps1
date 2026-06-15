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

.NOTES
    arm64/armhf builds run under emulation and can take many minutes each
    (aws-lc-rs + ring compile slowly under QEMU). This is expected.

.EXAMPLE
    .\ffi\docker\build-ffi-linux.ps1

.EXAMPLE
    .\ffi\docker\build-ffi-linux.ps1 -Only linux/arm64
#>
[CmdletBinding()]
param(
    [string]$RustVersion = "1.89.0",
    # Restrict to a subset, e.g. -Only linux/amd64,linux/arm64
    [string[]]$Only
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
$Targets = @(
    @{ Platform = "linux/amd64";  Dist = "linux-x86_64" },
    @{ Platform = "linux/arm64";  Dist = "linux-aarch64" },
    @{ Platform = "linux/arm/v7"; Dist = "linux-armhf" }
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

    # Build the per-platform toolchain image (layers cached after first run).
    Write-Host "[build image] $tag" -ForegroundColor DarkCyan
    Invoke-Native -What "docker build ($plat)" -Cmd {
        docker build --platform $plat `
            --build-arg "RUST_VERSION=$RustVersion" `
            -t $tag -f $Dockerfile $PSScriptRoot
    }

    # Clean compile inside the container (ephemeral target dir).
    Write-Host "[compile] $plat (this may take a while under emulation)" -ForegroundColor DarkCyan
    Invoke-Native -What "container build ($plat)" -Cmd {
        docker run --rm --platform $plat `
            -v "${RepoRoot}:/src" `
            -e "DIST=$dist" `
            $tag
    }

    # The container now emits a versioned layout: libint2dds_ffi.so.<ver> (real),
    # libint2dds_ffi.so.<major.minor> and libint2dds_ffi.so (soname/dev symlinks).
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
