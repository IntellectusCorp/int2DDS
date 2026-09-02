<#
.SYNOPSIS
    Verify the DT_SONAME embedded in the built libint2dds_ffi.so artifacts.

.DESCRIPTION
    Reads the ELF DT_SONAME dynamic tag directly (pure PowerShell, no Docker /
    WSL / binutils needed) from every libint2dds_ffi.so.<ver> produced by
    build-ffi-linux.ps1 under ffi/dist, and prints the soname per architecture.

    The soname is embedded by ffi/build.rs as libint2dds_ffi.so.<major>,
    derived from the workspace version in the repo-root Cargo.toml. For version
    0.1.3 the expected soname is therefore "libint2dds_ffi.so.0".

    Handles both 64-bit (x86_64, aarch64) and 32-bit (armhf) ELF objects.

.PARAMETER DistRoot
    Directory to scan. Defaults to ffi/dist next to this script.

.PARAMETER Path
    Check a single .so file instead of scanning DistRoot.

.EXAMPLE
    .\ffi\docker\check-soname.ps1

.EXAMPLE
    .\ffi\docker\check-soname.ps1 -Path .\ffi\dist\linux-x86_64\libint2dds_ffi.so.0.1.3
#>
[CmdletBinding()]
param(
    [string]$DistRoot,
    [string]$Path
)

$ErrorActionPreference = "Stop"

# Extract the DT_SONAME string from an ELF shared object. Returns $null when the
# object carries no SONAME. Throws on a non-ELF / big-endian / non-dynamic file.
function Get-ElfSoname {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$File)

    $b = [System.IO.File]::ReadAllBytes((Resolve-Path $File).Path)
    if ($b.Length -lt 64 -or $b[0] -ne 0x7F -or $b[1] -ne 0x45 -or $b[2] -ne 0x4C -or $b[3] -ne 0x46) {
        throw "Not an ELF file: $File"
    }
    if ($b[5] -ne 1) { throw "Big-endian ELF not supported by this helper: $File" }
    $is64 = ($b[4] -eq 2)

    if ($is64) {
        $shoff = [int][BitConverter]::ToUInt64($b, 0x28)
        $shent = [BitConverter]::ToUInt16($b, 0x3A)
        $shnum = [BitConverter]::ToUInt16($b, 0x3C)
    } else {
        $shoff = [int][BitConverter]::ToUInt32($b, 0x20)
        $shent = [BitConverter]::ToUInt16($b, 0x2E)
        $shnum = [BitConverter]::ToUInt16($b, 0x30)
    }

    # Locate SHT_DYNAMIC (type 6): file offset, size, and linked .dynstr index.
    $dynOff = $null; $dynSize = 0; $dynLink = 0
    for ($i = 0; $i -lt $shnum; $i++) {
        $sh = [int]($shoff + $i * $shent)
        if ([BitConverter]::ToUInt32($b, $sh + 4) -eq 6) {
            if ($is64) {
                $dynOff  = [int][BitConverter]::ToUInt64($b, $sh + 24)
                $dynSize = [int][BitConverter]::ToUInt64($b, $sh + 32)
                $dynLink = [int][BitConverter]::ToUInt32($b, $sh + 40)
            } else {
                $dynOff  = [int][BitConverter]::ToUInt32($b, $sh + 16)
                $dynSize = [int][BitConverter]::ToUInt32($b, $sh + 20)
                $dynLink = [int][BitConverter]::ToUInt32($b, $sh + 24)
            }
            break
        }
    }
    if ($null -eq $dynOff) { throw "No .dynamic section (not a shared object?): $File" }

    # .dynstr file offset comes from the linked section header.
    $shl = [int]($shoff + $dynLink * $shent)
    $strOff = if ($is64) { [int][BitConverter]::ToUInt64($b, $shl + 24) }
              else       { [int][BitConverter]::ToUInt32($b, $shl + 16) }

    # Walk dynamic entries for DT_SONAME (tag 14); stop at DT_NULL (tag 0).
    $ent = if ($is64) { 16 } else { 8 }
    for ($o = $dynOff; $o -lt $dynOff + $dynSize; $o += $ent) {
        if ($is64) { $tag = [BitConverter]::ToUInt64($b, $o); $val = [int][BitConverter]::ToUInt64($b, $o + 8) }
        else       { $tag = [BitConverter]::ToUInt32($b, $o); $val = [int][BitConverter]::ToUInt32($b, $o + 4) }
        if ($tag -eq 0)  { break }
        if ($tag -eq 14) {
            $s = [int]($strOff + $val); $e = $s
            while ($e -lt $b.Length -and $b[$e] -ne 0) { $e++ }
            return [System.Text.Encoding]::ASCII.GetString($b, $s, $e - $s)
        }
    }
    return $null
}

# --- single-file mode -------------------------------------------------------
if ($Path) {
    $soname = Get-ElfSoname $Path
    if ($soname) { Write-Host "SONAME: $soname  <- $Path" -ForegroundColor Green }
    else         { Write-Host "NO SONAME embedded  <- $Path" -ForegroundColor Red }
    return
}

# --- scan mode --------------------------------------------------------------
if (-not $DistRoot) { $DistRoot = Join-Path $PSScriptRoot "..\dist" }
if (-not (Test-Path $DistRoot)) {
    throw "Dist directory not found: $DistRoot. Run build-ffi-linux.ps1 first."
}

# Real artifacts are libint2dds_ffi.so.<major>.<minor>.<patch> (the .so and
# .so.<major> entries are symlinks). Match the three-segment version form.
$files = Get-ChildItem $DistRoot -Recurse -File |
    Where-Object { $_.Name -match '^libint2dds_ffi\.so\.\d+\.\d+\.\d+$' }

if (-not $files) {
    throw "No libint2dds_ffi.so.<ver> files under $DistRoot. Run build-ffi-linux.ps1 first."
}

$rows = foreach ($f in $files) {
    $soname = $null; $err = $null
    try { $soname = Get-ElfSoname $f.FullName } catch { $err = $_.Exception.Message }
    [pscustomobject]@{
        Arch   = Split-Path (Split-Path $f.FullName -Parent) -Leaf
        File   = $f.Name
        SONAME = if ($soname) { $soname } elseif ($err) { "ERROR: $err" } else { "(none)" }
    }
}

$rows | Format-Table -AutoSize

if ($rows | Where-Object { $_.SONAME -notmatch '^libint2dds_ffi\.so\.\d+$' }) {
    Write-Host "WARNING: one or more artifacts have a missing/unexpected SONAME." -ForegroundColor Red
    exit 1
}
Write-Host "All artifacts carry a versioned SONAME." -ForegroundColor Green
