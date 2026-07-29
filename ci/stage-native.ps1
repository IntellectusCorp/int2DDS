<#
.SYNOPSIS
    Stage the built Windows native artifacts into a release archive.
.EXAMPLE
    ci/stage-native.ps1 -Triple x86_64-pc-windows-msvc -DistName windows-x86_64 -Version 0.1.1
#>
param(
    [Parameter(Mandatory)][string]$Triple,
    [Parameter(Mandatory)][string]$DistName,
    [Parameter(Mandatory)][string]$Version
)
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$BuildDir = Join-Path $RepoRoot "target\$Triple\release"
$Stage    = Join-Path $RepoRoot "dist\stage\int2dds-$Version-$DistName"
$OutDir   = Join-Path $RepoRoot "dist"

if (Test-Path $Stage) { Remove-Item -Recurse -Force $Stage }
foreach ($d in 'bin','lib','include','src') {
    New-Item -ItemType Directory -Force -Path (Join-Path $Stage $d) | Out-Null
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$dll = Join-Path $BuildDir 'int2dds_ffi.dll'
if (-not (Test-Path $dll)) { throw "int2dds_ffi.dll not found at $dll" }
Copy-Item $dll (Join-Path $Stage 'bin\int2dds_ffi.dll')

$exe = Join-Path $BuildDir 'int2dds-idl.exe'
if (-not (Test-Path $exe)) { throw "int2dds-idl.exe not found at $exe" }
Copy-Item $exe (Join-Path $Stage 'bin\int2dds-idl.exe')

# import lib: int2dds_ffi.dll.lib on MSVC, libint2dds_ffi.dll.a on MinGW.
# It is a DLL entry-point stub, not a static library; C integration is impossible
# without it.
$importLib = if ($Triple -like '*-gnu') { 'libint2dds_ffi.dll.a' } else { 'int2dds_ffi.dll.lib' }
$importPath = Join-Path $BuildDir $importLib
if (-not (Test-Path $importPath)) { throw "import library not found at $importPath" }
Copy-Item $importPath (Join-Path $Stage "lib\$importLib")

# PDBs are produced for MSVC targets only; their absence is normal on MinGW.
$pdb = Join-Path $BuildDir 'int2dds_ffi.pdb'
if (Test-Path $pdb) {
    Copy-Item $pdb (Join-Path $Stage 'bin\int2dds_ffi.pdb')
} elseif ($Triple -notlike '*-gnu') {
    throw "int2dds_ffi.pdb not found for MSVC target $Triple"
}

Copy-Item (Join-Path $RepoRoot 'ffi\include\int2dds-ffi.h') (Join-Path $Stage 'include\')
Copy-Item (Join-Path $RepoRoot 'ffi\include\int2dds_cdr.h') (Join-Path $Stage 'include\')
Copy-Item (Join-Path $RepoRoot 'ffi\src\cdr_utils.c')       (Join-Path $Stage 'src\')
foreach ($f in 'LICENSE','NOTICE','Third_Party_Licenses.md') {
    Copy-Item (Join-Path $RepoRoot $f) $Stage
}

$sha = (Get-FileHash (Join-Path $Stage 'bin\int2dds_ffi.dll') -Algorithm SHA256).Hash.ToLower()
$fileVersion = (Get-Item (Join-Path $Stage 'bin\int2dds_ffi.dll')).VersionInfo.FileVersion
@(
    "name: int2dds"
    "version: `"$Version`""
    "api_header: int2dds-ffi.h"
    "license: Apache-2.0"
    "triple: $Triple"
    "dist: $DistName"
    "sha256: $sha"
    "import_lib: $importLib"
    "pe_file_version: `"$fileVersion`""
) | Set-Content -Path (Join-Path $Stage 'manifest.txt') -Encoding utf8

$archive = Join-Path $OutDir "int2dds-$Version-$DistName.zip"
if (Test-Path $archive) { Remove-Item -Force $archive }
Compress-Archive -Path $Stage -DestinationPath $archive

Write-Host "== manifest =="
Get-Content (Join-Path $Stage 'manifest.txt')
Write-Host "== archive =="
Expand-Archive -Path $archive -DestinationPath (Join-Path $env:TEMP "verify-$DistName") -Force
Get-ChildItem -Recurse (Join-Path $env:TEMP "verify-$DistName") | Select-Object -ExpandProperty FullName
Write-Output $archive
