<#
.SYNOPSIS
    Build int2dds_ffi.dll for Windows targets natively and package a .zip.

.DESCRIPTION
    Builds a versioned Windows distribution:
        ffi/dist/int2dds-ffi-<version>-windows.zip
        ├── int2dds-ffi.manifest.yaml
        ├── int2dds-ffi.h
        ├── LICENSE
        ├── windows-x86_64/int2dds_ffi.dll      (+ import lib)
        ├── windows-i686/int2dds_ffi.dll
        ├── windows-aarch64/int2dds_ffi.dll      (cross; skipped if toolchain absent)
        └── windows-x86_64-gnu/int2dds_ffi.dll   (MinGW; skipped if toolchain absent)

    The DLL filename is unversioned (Windows convention); the version lives in the
    embedded PE VERSIONINFO resource (see ffi/build.rs). Version comes from the
    workspace Cargo.toml. Targets whose toolchain is missing are skipped with a
    message, never silently dropped.

.EXAMPLE
    .\ffi\docker\build-ffi-windows.ps1

.EXAMPLE
    .\ffi\docker\build-ffi-windows.ps1 -Only x86_64-pc-windows-msvc -NoPackage
#>
[CmdletBinding()]
param(
    [string[]]$Only,
    [switch]$NoPackage
)

$ErrorActionPreference = "Stop"

# triple -> (dist subdir, debian-ish arch, runtime)
$Targets = @(
    @{ Triple = "x86_64-pc-windows-msvc";  Dist = "windows-x86_64";     Arch = "amd64"; Runtime = "msvc" },
    @{ Triple = "i686-pc-windows-msvc";    Dist = "windows-i686";       Arch = "x86";   Runtime = "msvc" },
    @{ Triple = "aarch64-pc-windows-msvc"; Dist = "windows-aarch64";    Arch = "arm64"; Runtime = "msvc" },
    @{ Triple = "x86_64-pc-windows-gnu";   Dist = "windows-x86_64-gnu"; Arch = "amd64"; Runtime = "gnu"  }
)
if ($Only) { $Targets = $Targets | Where-Object { $Only -contains $_.Triple } }
if (-not $Targets) { throw "No targets selected (check -Only values)." }

$RepoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$DistRoot = Join-Path $RepoRoot "ffi\dist"
$Header   = Join-Path $RepoRoot "ffi\include\int2dds-ffi.h"

Write-Host "Repo root : $RepoRoot"
Write-Host "Targets   : $($Targets.Triple -join ', ')"

# Version = workspace SSOT.
$ver = (Select-String -Path (Join-Path $RepoRoot "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' |
        Select-Object -First 1).Matches.Groups[1].Value
if (-not $ver) { throw "Could not read version from root Cargo.toml" }
Write-Host "Version   : $ver"

# Clean only the windows-* dist subdirs (leave linux/macos artifacts intact).
foreach ($t in $Targets) {
    $p = Join-Path $DistRoot $t.Dist
    if (Test-Path $p) { Remove-Item -Recurse -Force $p }
}
New-Item -ItemType Directory -Force -Path $DistRoot | Out-Null

$results = @()
foreach ($t in $Targets) {
    Write-Host "`n===== $($t.Triple) -> ffi/dist/$($t.Dist) =====" -ForegroundColor Cyan

    # Ensure the rustup target is installed (no-op if present).
    rustup target add $t.Triple | Out-Null

    # Build; capture failure so a missing cross toolchain skips this target.
    $built = $true
    & cargo build --release --target $t.Triple -p int2dds-ffi
    if ($LASTEXITCODE -ne 0) {
        Write-Host "[skip] $($t.Triple): build failed (toolchain missing?)" -ForegroundColor Yellow
        $built = $false
    }
    if (-not $built) { continue }

    $relDir = Join-Path $RepoRoot "target\$($t.Triple)\release"
    $dll = Join-Path $relDir "int2dds_ffi.dll"
    if (-not (Test-Path $dll)) { throw "Expected DLL missing: $dll" }

    $outDir = Join-Path $DistRoot $t.Dist
    New-Item -ItemType Directory -Force -Path $outDir | Out-Null
    Copy-Item $dll (Join-Path $outDir "int2dds_ffi.dll")

    # Import library: MSVC -> int2dds_ffi.dll.lib ; GNU -> libint2dds_ffi.dll.a
    foreach ($implib in @("int2dds_ffi.dll.lib", "libint2dds_ffi.dll.a")) {
        $p = Join-Path $relDir $implib
        if (Test-Path $p) { Copy-Item $p (Join-Path $outDir $implib) }
    }

    $sha = (Get-FileHash (Join-Path $outDir "int2dds_ffi.dll") -Algorithm SHA256).Hash.ToLower()
    $fileVer = (Get-Item (Join-Path $outDir "int2dds_ffi.dll")).VersionInfo.FileVersion
    $results += [pscustomobject]@{
        Triple = $t.Triple; Dist = $t.Dist; Arch = $t.Arch; Runtime = $t.Runtime
        Sha256 = $sha; FileVersion = $fileVer
    }
}

if (-not $results) { throw "No Windows targets built successfully." }

Write-Host "`n===== Built =====" -ForegroundColor Green
$results | Format-Table Triple, FileVersion, @{n='SizeKB';e={[math]::Round((Get-Item (Join-Path $DistRoot "$($_.Dist)\int2dds_ffi.dll")).Length/1KB,1)}} -AutoSize

if ($NoPackage) { return }

# --- Package: int2dds-ffi-<ver>-windows.zip --------------------------------
Write-Host "`n===== Packaging archive =====" -ForegroundColor Cyan
$commit = (& git -C $RepoRoot rev-parse --short HEAD).Trim()
$date   = Get-Date -Format 'yyyy-MM-dd'

$stage = Join-Path $env:TEMP "int2dds-ffi-$ver-windows"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

Copy-Item $Header (Join-Path $stage "int2dds-ffi.h")
Copy-Item (Join-Path $RepoRoot "LICENSE") (Join-Path $stage "LICENSE")

$manifest = Join-Path $stage "int2dds-ffi.manifest.yaml"
$lines = @(
    "name: int2dds-ffi"
    "version: $ver"
    "git_commit: $commit"
    "build_date: `"$date`""
    "api_header: int2dds-ffi.h"
    "license: Apache-2.0"
    "artifacts:"
)
foreach ($r in $results) {
    $archDir = Join-Path $stage $r.Dist
    New-Item -ItemType Directory -Force -Path $archDir | Out-Null
    Copy-Item (Join-Path $DistRoot "$($r.Dist)\int2dds_ffi.dll") (Join-Path $archDir "int2dds_ffi.dll")
    $lines += @(
        "  - os: windows"
        "    arch: $($r.Arch)"
        "    triple: $($r.Triple)"
        "    file: $($r.Dist)/int2dds_ffi.dll"
        "    sha256: $($r.Sha256)"
        "    runtime: $($r.Runtime)"
        "    min_windows: `"10`""
    )
}
$lines | Set-Content -Path $manifest -Encoding utf8

$zip = Join-Path $DistRoot "int2dds-ffi-$ver-windows.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $zip

Write-Host "`n== manifest =="; Get-Content $manifest
Write-Host "`nArchive : ffi/dist/int2dds-ffi-$ver-windows.zip" -ForegroundColor Green
