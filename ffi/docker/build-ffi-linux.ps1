<#
.SYNOPSIS
    Clean-build libint2dds_ffi.so for all Linux targets.

.DESCRIPTION
    Produces forward-compatible Linux shared libraries for every
    architecture the RMW layer needs, in one run:

        x86_64   -> ffi/dist/linux-x86_64/libint2dds_ffi.so        (glibc 2.28, AlmaLinux 8)
        arm64    -> ffi/dist/linux-aarch64/libint2dds_ffi.so       (glibc 2.28, AlmaLinux 8)
        armhf    -> ffi/dist/linux-armhf/libint2dds_ffi.so         (glibc 2.35, Ubuntu 22.04, 32-bit ARM)

    x86_64/arm64 are built on manylinux_2_28 (AlmaLinux 8, glibc 2.28) so one
    artifact per arch covers RHEL 8/9/10 and Ubuntu 20.04/22.04/24.04. armhf
    has no manylinux image, so it stays on Ubuntu 22.04 (glibc 2.35).

    Each architecture is compiled NATIVELY inside its own-arch container
    (amd64 native; arm64/armhf via Docker Desktop's QEMU emulation), so the
    ring crypto crate builds the same way it would on real
    hardware. This is slower than cross-linking but reliable.

    All selected targets build CONCURRENTLY as background jobs (-Jobs throttles
    it). They share only the bind-mounted source tree; each writes to its own
    ffi/dist/<arch> subdir with a container-local CARGO_TARGET_DIR, so nothing
    races. Five interleaved live build logs would be unreadable, so each target
    streams to ffi/dist/logs/<arch>.log and only start/finish lines are printed.

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
    (ring compiles slowly under QEMU). This is expected. Because targets run
    concurrently the wall clock is the slowest single target, not the sum.

.EXAMPLE
    .\ffi\docker\build-ffi-linux.ps1

.EXAMPLE
    .\ffi\docker\build-ffi-linux.ps1 -Only linux/arm64

.EXAMPLE
    .\ffi\docker\build-ffi-linux.ps1 -NoPackage   # build only, skip the tarball

.EXAMPLE
    .\ffi\docker\build-ffi-linux.ps1 -Jobs 1      # sequential, easier to watch
#>
[CmdletBinding()]
param(
    [string]$RustVersion = "1.89.0",
    # Restrict to a subset, e.g. -Only linux/amd64,linux/arm64
    [string[]]$Only,
    # Skip assembling the .tar.gz distribution archive.
    [switch]$NoPackage,
    # Build at most this many targets concurrently. 0 = all of them at once.
    [int]$Jobs = 0,
    # tonistiigi/binfmt QEMU tag to register. PINNED, not :latest -- see the
    # binfmt section below before changing it.
    [string]$QemuVersion = "v8.1.5"
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

# target triple-ish platform -> (docker platform, dist subdir, Dockerfile, base image)
# x86_64/aarch64 gnu build from manylinux_2_28 (AlmaLinux 8, glibc 2.28) so one
# artifact per arch covers RHEL 8/9/10 and Ubuntu 20.04+. manylinux has no armv7
# image, so armhf keeps its own Ubuntu 22.04 Dockerfile at floor 2.35. Musl
# targets build from the Alpine Dockerfile.musl (native musl). Each is compiled
# in its own-arch container.
$Targets = @(
    @{ Platform = "linux/amd64";  Dist = "linux-x86_64";       File = "Dockerfile";       Base = "quay.io/pypa/manylinux_2_28_x86_64"  },
    @{ Platform = "linux/arm64";  Dist = "linux-aarch64";      File = "Dockerfile";       Base = "quay.io/pypa/manylinux_2_28_aarch64" },
    @{ Platform = "linux/arm/v7"; Dist = "linux-armhf";        File = "Dockerfile.armhf"; Base = $null },
    @{ Platform = "linux/amd64";  Dist = "linux-x86_64-musl";  File = "Dockerfile.musl";  Base = $null },
    @{ Platform = "linux/arm64";  Dist = "linux-aarch64-musl"; File = "Dockerfile.musl";  Base = $null }
)
if ($Only) { $Targets = $Targets | Where-Object { $Only -contains $_.Platform } }
if (-not $Targets) { throw "No targets selected (check -Only values)." }

$RepoRoot   = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$DistRoot   = Join-Path $RepoRoot "ffi\dist"

Write-Host "Repo root : $RepoRoot"
Write-Host "Targets   : $($Targets.Platform -join ', ')"

# Docker engine reachable?
docker info --format '{{.ServerVersion}}' 2>$null | Out-Null
if (-not $?) { throw "Docker engine not reachable. Start Docker Desktop and retry." }

# Register a KNOWN-GOOD QEMU. Two separate traps live here, and the fix for one
# is not the fix for the other:
#
#   1. Too OLD an emulator dies early and loudly -- exit 139 out of
#      ldconfig/libc-bin during an emulated apt install, or "QEMU internal
#      SIGSEGV" while rustup-init is still downloading.
#
#   2. Too NEW an emulator dies late and silently. Measured on the bash side of
#      this repo, gnu `rustc --version` under qemu-user: v7.0.0 and v8.1.5 OK,
#      v9.2.2 and v10.2.3 HANG. rustc's bundled jemalloc probes MADV_DONTNEED;
#      old QEMU reports it unsupported so jemalloc takes its memset fallback,
#      while newer QEMU claims a support it does not deliver and jemalloc then
#      hangs. musl targets carry no jemalloc and are unaffected.
#
# So $QemuVersion is PINNED rather than tracking :latest. Raising it requires
# re-running an emulated gnu target end to end, not just a smoke command.
# Registration is not persistent across Docker Desktop restarts, so we (re)apply
# it each run.
#
# ALWAYS uninstall before installing: --install SKIPS an arch that is already
# registered, it never replaces one. Where a registration already exists a bare
# --install is a silent no-op and the build keeps using the old emulator, which
# does not fail here but minutes later. Uninstall failure is not fatal -- having
# nothing to remove is the normal case on a fresh machine.
$binfmtImage = "tonistiigi/binfmt:qemu-$QemuVersion"
$needsEmulation = $Targets | Where-Object { $_.Platform -ne "linux/amd64" }
if ($needsEmulation) {
    Write-Host "`nRefreshing QEMU binfmt emulators (arm64, arm) from $binfmtImage ..." -ForegroundColor Yellow
    Write-Host "  clearing any existing registration first" -ForegroundColor DarkYellow
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try { docker run --privileged --rm $binfmtImage --uninstall arm64,arm *>&1 | Out-Null }
    finally { $ErrorActionPreference = $prevEap }
    if ($LASTEXITCODE -ne 0) { Write-Host "  (nothing registered to clear)" -ForegroundColor DarkYellow }
    Invoke-Native -What "binfmt install" -Cmd {
        docker run --privileged --rm $binfmtImage --install arm64,arm
    }
}

# Always clean: wipe previous artifacts.
if (Test-Path $DistRoot) {
    Write-Host "`nCleaning $DistRoot ..." -ForegroundColor Yellow
    Remove-Item -Recurse -Force $DistRoot
}
New-Item -ItemType Directory -Force -Path $DistRoot | Out-Null

# One target's whole job: build its image, compile in it, locate the artifact.
# Runs as a background job, so it cannot touch the caller's variables -- every
# input arrives through -ArgumentList and the outcome comes back as the single
# object it returns. All docker chatter goes to the target's own log file;
# returning it instead would interleave five builds into one unreadable stream.
$targetJob = {
    param($Plat, $Dist, $DockerFile, $Base, $RustVersion, $ScriptRoot, $RepoRoot, $DistRoot, $CargoJobs, $LogPath)

    # BuildKit writes progress to stderr; without this PowerShell would wrap each
    # line as a terminating error. Success is gated on the exit code instead.
    $ErrorActionPreference = "Continue"

    $tag = "int2dds-ffi-builder:$Dist"
    $df  = Join-Path $ScriptRoot $DockerFile
    $fail = { param($m) [pscustomobject]@{ Dist = $Dist; Ok = $false; Message = $m } }

    # manylinux names its image per arch, so the gnu x86_64/aarch64 targets pass
    # BASE_IMAGE. The armhf and musl Dockerfiles pin their own FROM and take none.
    $buildArgs = @("--build-arg", "RUST_VERSION=$RustVersion")
    if ($Base) { $buildArgs += @("--build-arg", "BASE_IMAGE=$Base") }

    "===== $Plat -> ffi/dist/$Dist =====" | Out-File -FilePath $LogPath -Encoding utf8
    "[build image] $tag  (from $DockerFile)" | Out-File -FilePath $LogPath -Append -Encoding utf8
    docker build --platform $Plat @buildArgs -t $tag -f $df $ScriptRoot *>&1 |
        Out-File -FilePath $LogPath -Append -Encoding utf8
    if ($LASTEXITCODE -ne 0) { return (& $fail "docker build failed (exit $LASTEXITCODE)") }

    "[compile] $Plat (this may take a while under emulation)" | Out-File -FilePath $LogPath -Append -Encoding utf8
    docker run --rm --platform $Plat `
        -v "${RepoRoot}:/src" `
        -e "DIST=$Dist" `
        -e "CARGO_BUILD_JOBS=$CargoJobs" `
        $tag *>&1 | Out-File -FilePath $LogPath -Append -Encoding utf8
    if ($LASTEXITCODE -ne 0) { return (& $fail "container build failed (exit $LASTEXITCODE)") }

    # The container now emits a versioned layout: libint2dds_ffi.so.<ver> (real),
    # libint2dds_ffi.so.<major> and libint2dds_ffi.so (soname/dev symlinks).
    $soLink = Join-Path $DistRoot "$Dist\libint2dds_ffi.so"
    if (-not (Test-Path $soLink)) { return (& $fail "artifact missing at $soLink") }
    # Real file is the largest libint2dds_ffi.so.* (the symlinks are tiny).
    $real = Get-ChildItem (Join-Path $DistRoot $Dist) -Filter 'libint2dds_ffi.so.*' |
            Sort-Object Length -Descending | Select-Object -First 1
    if (-not $real) { return (& $fail "no real .so under ffi/dist/$Dist") }

    [pscustomobject]@{
        Dist     = $Dist
        Ok       = $true
        Message  = ""
        Platform = $Plat
        Output   = "ffi/dist/$Dist/$($real.Name)"
        SizeMB   = [math]::Round($real.Length / 1MB, 2)
    }
}

$LogDir = Join-Path $DistRoot "logs"
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

$maxJobs = if ($Jobs -gt 0) { [math]::Min($Jobs, $Targets.Count) } else { $Targets.Count }

# Cap each container's rustc fan-out. Cargo otherwise defaults to one job per
# host CPU *in every container at once*, so N concurrent targets oversubscribe
# the machine N-fold; rustc peaks near 1 GB per process, so on a wide box that is
# an OOM waiting to happen rather than a speed-up.
$cargoJobs = [math]::Max(2, [int][math]::Floor([Environment]::ProcessorCount / $maxJobs))

Write-Host "`nBuilding $($Targets.Count) target(s), up to $maxJobs at a time, $cargoJobs cargo job(s) each" -ForegroundColor Cyan
Write-Host "Logs      : ffi/dist/logs/<target>.log"

$running = @()
foreach ($t in $Targets) {
    # Throttle to $maxJobs concurrent targets. Only our own jobs are counted --
    # the session may hold unrelated ones.
    while (@($running | Where-Object { $_.State -eq 'Running' }).Count -ge $maxJobs) {
        Start-Sleep -Seconds 2
    }
    Write-Host "  start    $($t.Dist)" -ForegroundColor DarkCyan
    $running += Start-Job -ScriptBlock $targetJob -ArgumentList @(
        $t.Platform, $t.Dist, $t.File, $t.Base, $RustVersion,
        $PSScriptRoot, $RepoRoot, $DistRoot, $cargoJobs,
        (Join-Path $LogDir "$($t.Dist).log")
    )
}

$null = Wait-Job -Job $running
$outcomes = @($running | ForEach-Object { Receive-Job -Job $_ })
Remove-Job -Job $running

foreach ($o in $outcomes) {
    if ($o.Ok) { Write-Host "  ok       $($o.Dist)" -ForegroundColor DarkGreen }
    else       { Write-Host "  FAILED   $($o.Dist) - $($o.Message)" -ForegroundColor Red }
}

$failed = @($outcomes | Where-Object { -not $_.Ok })
if ($failed) {
    foreach ($f in $failed) {
        $log = Join-Path $LogDir "$($f.Dist).log"
        Write-Host "`n===== FAILED: $($f.Dist) - last 40 lines of ffi/dist/logs/$($f.Dist).log =====" -ForegroundColor Red
        if (Test-Path $log) { Get-Content $log -Tail 40 } else { Write-Host "(no log written)" }
    }
    throw "build failed for: $(($failed.Dist) -join ', ')"
}

$results = $outcomes | Select-Object Platform, Output, SizeMB

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
