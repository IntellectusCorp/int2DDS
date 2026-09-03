# FFI native builds (Linux / Windows / macOS)

Build scripts that produce the distributable `int2dds-ffi` native libraries.
Every platform has a **`.sh` (bash)** entry point; the Linux and Windows ones
also keep their original **`.ps1` (PowerShell)** twin. The pairs build the
same targets into the same `ffi/dist` layout with the same manifest fields,
but their `--only`/`-Only` flags do not accept identical values (Linux is
the case that differs — see below), so pick whichever matches the shell you
are in and check the flag details before scripting against it.

| Platform     | bash                   | PowerShell              | Needs                 |
| ------------ | ---------------------- | ----------------------- | --------------------- |
| Linux        | `build-ffi-linux.sh`   | `build-ffi-linux.ps1`   | Docker                |
| Windows      | `build-ffi-windows.sh` | `build-ffi-windows.ps1` | cargo + toolchain     |
| macOS        | `build-ffi-macos.sh`   | —                       | cargo + Xcode CLT     |
| SONAME check | `check-soname.sh`      | `check-soname.ps1`      | nothing (or binutils) |

---

## Linux — `build-ffi-linux.sh`

Clean-builds `libint2dds_ffi.so` for every Linux architecture the RMW layer
needs, in one command. The x86_64 and arm64 glibc targets are built against
**glibc 2.28** (AlmaLinux 8, via `quay.io/pypa/manylinux_2_28_*`); armhf against
**glibc 2.35** (Ubuntu 22.04); musl targets against **Alpine 3.20**.

```bash
./ffi/docker/build-ffi-linux.sh                      # all 5 targets + tarball
./ffi/docker/build-ffi-linux.sh --only linux/arm64   # gnu + musl arm64
./ffi/docker/build-ffi-linux.sh --only linux-x86_64  # a single dist dir
./ffi/docker/build-ffi-linux.sh --no-package         # build only, no tarball
./ffi/docker/build-ffi-linux.sh --jobs 1             # sequential, easier to watch
```

| Flag                 | Effect                                                                    |
| -------------------- | ------------------------------------------------------------------------- |
| `--only <list>`      | Subset by docker platform (`linux/amd64`) **or** dist dir (`linux-armhf`) |
| `--rust-version <x>` | Override the toolchain (default `1.89.0`)                                 |
| `--qemu-version <x>` | QEMU tag to register (default `v8.1.5`) — see the emulator gotcha below  |
| `--no-package`       | Skip the `.tar.gz` distribution archive                                   |
| `--jobs <n>`         | Build at most n targets at once (default: all of them; `1` = sequential)  |
| `--no-binfmt`        | Skip the privileged QEMU binfmt registration container                    |
| `--refresh-binfmt`   | Accepted and ignored — refreshing is the default (see below)              |

### Why one glibc floor instead of one build per OS

glibc is forward-compatible: a binary linked against an older glibc runs on
newer ones, but not vice-versa. And this library has **no `libstdc++`
dependency** — `rustls` is pinned to the `ring` feature rather than `aws-lc`,
so nothing here is C++ and the compiler version does not affect artifact
compatibility.

That leaves the glibc floor as the only variable. Building at the lowest one
covers everything above it, so we ship **one `.so` per arch, not one per
distro**:

| Target | glibc | Covered |
| ------ | ----- | ------- |
| RHEL 8 | 2.28 | yes |
| Ubuntu 20.04 | 2.31 | yes |
| RHEL 9 | 2.34 | yes |
| Ubuntu 22.04 | 2.35 | yes |
| Ubuntu 24.04 | 2.39 | yes |

`manylinux_2_28` is AlmaLinux 8, which is the RHEL 8 ABI itself, and PyPI keeps
it maintained — building our own CentOS-era image would mean owning an EOL
package repo.

**armhf is the exception.** manylinux publishes no armv7 image, and every
lower-glibc armhf builder is unmaintained (debian 10 archived, debian 11 LTS
ends 2026-08-31, ubuntu 20.04 EOL). armhf therefore keeps its own
`Dockerfile.armhf` (Ubuntu 22.04) at floor 2.35, which is where it already was.
This is a deferred improvement, not a regression: armhf has no RHEL and is not
a ROS 2 tier 1 platform.

| Dockerfile | Targets | Base | Floor |
| ---------- | ------- | ---- | ----- |
| `Dockerfile` | `linux/amd64`, `linux/arm64` | `quay.io/pypa/manylinux_2_28_*` | 2.28 |
| `Dockerfile.armhf` | `linux/arm/v7` | `ubuntu:22.04` | 2.35 |
| `Dockerfile.musl` | `linux/amd64`, `linux/arm64` | `alpine:3.20` | musl, no glibc |

Both `build-ffi-linux.sh` and `build-ffi-linux.ps1` carry this table in their
target lists, but `--only`/`-Only` do not accept the same values on both: the
bash script accepts docker platforms (`linux/amd64`) or dist dir names
(`linux-x86_64`), while the PowerShell script accepts docker platforms only —
passing it a dist dir name throws "No targets selected".

### Output layout

```
ffi/dist/
  int2dds-ffi.h                          # architecture-independent C header
  int2dds-ffi-<ver>-linux.tar.gz         # distribution archive (+ manifest)
  linux-x86_64/                          # ELF64 x86-64        (glibc)
  linux-aarch64/                         # ELF64 AArch64       (glibc)
  linux-armhf/                           # ELF32 ARM v7 hf     (glibc)
  linux-x86_64-musl/                     # ELF64 x86-64        (musl)
  linux-aarch64-musl/                    # ELF64 AArch64       (musl)
```

Each arch dir carries the standard three-tier ELF layout:

```
libint2dds_ffi.so         -> libint2dds_ffi.so.<major>   (dev symlink)
libint2dds_ffi.so.<major> -> libint2dds_ffi.so.<ver>     (soname symlink)
libint2dds_ffi.so.<ver>                                  (real file)
```

`ffi/dist/` is git-ignored. Every run is a **clean build**: `ffi/dist` is wiped
first and the in-container Cargo target dir is ephemeral.

### How it works

- Each architecture is compiled **natively inside its own-arch container**
  (`docker build/run --platform ...`): the host's own arch runs native, the rest
  run under QEMU emulation. This keeps the `ring` crypto crate
  (cmake + C/asm) building exactly as on real hardware, which is more reliable
  than cross-linking.
- All selected targets build **concurrently** (`--jobs` throttles it). The only
  host paths a container writes are its own `ffi/dist/<arch>` subdir and the
  arch-independent `int2dds-ffi.h`, which every container copies from the same
  source file — so the bytes are identical whoever writes last. `cargo` runs
  `--locked` so no container can rewrite the shared `Cargo.lock`.
- Because five interleaved live build logs are unreadable, each target streams
  to `ffi/dist/logs/<arch>.log` and only start/finish lines reach the console.
  A failed target gets the tail of its log dumped before the script exits.
- `CARGO_BUILD_JOBS` is set to `nproc / <concurrent targets>`. Without it every
  container would default to one rustc job per host CPU *simultaneously*, and
  rustc peaking near 1 GB per process turns that into an OOM rather than a
  speed-up.
- The repo is bind-mounted at `/src`; only the toolchain lives in the image, so
  code changes need no image rebuild.
- `CARGO_TARGET_DIR` is an ephemeral container path, so the host's `target/`
  directory is never touched and every run is clean.
- After each compile the container prints the **highest required GLIBC symbol
  version** — for the x86_64/arm64 `manylinux_2_28` builds confirm it is
  `<= GLIBC_2.28`; armhf builds separately in its own `Dockerfile.armhf` at the
  2.35 floor it already had (see the table above). musl builds require no
  GLIBC symbols at all.
- The bash script additionally detects the host arch, registers QEMU emulators
  only for the platforms that actually need them, and `chown`s the
  container-written artifacts back to the invoking user (a Linux bind-mount
  detail the Docker Desktop path on Windows/macOS does not have).

### Build time

Emulated builds are slow. Expect roughly:

| Target | Mode   | Approx compile time |
| ------ | ------ | ------------------- |
| native | native | ~3 min              |
| arm64  | QEMU   | ~15 min             |
| armhf  | QEMU   | ~23 min             |

Targets run concurrently, so the wall clock for a full run is the **slowest
single target**, not the sum — roughly the armhf figure above, plus contention.

### Gotchas handled by this setup

- **Emulated-guest QEMU failures.** Two opposite traps, and the fix for one is
  not the fix for the other.

  *Too old* dies early and loudly: `libc-bin`/`ldconfig` segfaulting during apt
  (exit 139), an apt GPG error, or `QEMU internal SIGSEGV` out of `rustup-init`.
  `tonistiigi/binfmt --install` **skips** an arch that is already registered
  rather than replacing it, so on a host carrying distro registrations from
  `binfmt-support`/`qemu-user-static` a bare install is a silent no-op and the
  build quietly keeps using the distro emulator. The script therefore
  **uninstalls before installing on every run**, then reads
  `/proc/sys/fs/binfmt_misc` to confirm the kernel now points at
  `/usr/bin/qemu-*` rather than a distro path.

  *Too new* dies late and silently. Measured here, x86_64 gnu `rustc --version`
  under qemu-user:

  | QEMU | Result |
  | ---- | ------ |
  | `v7.0.0`  | OK — warns `MADV_DONTNEED does not work`, jemalloc falls back |
  | `v8.1.5`  | OK |
  | `v9.2.2`  | **hangs** |
  | `v10.2.3` | **hangs** |

  rustc's bundled jemalloc probes `MADV_DONTNEED`. Old QEMU reports it
  unsupported, so jemalloc takes its memset fallback and lives; newer QEMU
  claims a support it does not deliver, so jemalloc commits to a path that then
  hangs. musl targets carry no jemalloc, which is why musl x86_64 builds fine on
  a QEMU that cannot build gnu x86_64. The version is therefore **pinned**
  (`--qemu-version`), not tracked to `:latest` — raise it only after re-running
  an emulated **gnu** target end to end, never on a smoke command alone.

  Registration changes are **host-wide**, not just for Docker; hand the machine
  back to its own emulators with `sudo systemctl restart systemd-binfmt` (or
  `sudo update-binfmts --enable`). Use `--no-binfmt` to skip the section.
- **SIGPIPE (exit 141)** under emulation — diagnostic pipes like `ldd | head`
  raise SIGPIPE when the reader closes early; the container script avoids
  `pipefail` so these never abort the build.
- **32-bit `off_t`** — `ftruncate` takes `i32` on armhf vs `i64` on 64-bit;
  fixed in `dds/src/rtps/transport/shm/platform/unix.rs` by casting to
  `libc::off_t` instead of a fixed `i64`.

### Docker prerequisites

`docker info` must succeed for the current user. On Ubuntu/Debian:

```bash
# Docker Engine from Docker's own apt repo (preferred over the distro docker.io)
sudo apt-get update
sudo apt-get install -y ca-certificates curl
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] \
  https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo $VERSION_CODENAME) stable" \
  | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
sudo apt-get update
sudo apt-get install -y docker-ce docker-ce-cli containerd.io \
                        docker-buildx-plugin docker-compose-plugin

# Run docker without sudo (log out / newgrp to apply)
sudo usermod -aG docker "$USER" && newgrp docker

docker info --format '{{.ServerVersion}}'   # verify
```

Multi-arch emulation needs `binfmt_misc`; the script installs the emulators
itself via `docker run --privileged tonistiigi/binfmt`, which requires the
privileged flag to be permitted. If it is not, install them once out of band
and pass `--no-binfmt`. Note that the script replaces any existing registration
for the arches it needs, with a pinned QEMU version — see the emulator gotcha
above for why, and for how to restore the distro's own.

---

## Windows — `build-ffi-windows.sh`

Builds `int2dds_ffi.dll` and packages `ffi/dist/int2dds-ffi-<ver>-windows.zip`.
This one does **not** use Docker — it drives `cargo`/`rustup` directly.

```bash
./ffi/docker/build-ffi-windows.sh
./ffi/docker/build-ffi-windows.sh --only x86_64-pc-windows-gnu --no-package
```

| Target                    | Where it builds                                       |
| ------------------------- | ----------------------------------------------------- |
| `x86_64-pc-windows-msvc`  | Windows only (Git Bash / MSYS2 + MSVC build tools)    |
| `i686-pc-windows-msvc`    | Windows only                                          |
| `aarch64-pc-windows-msvc` | Windows only                                          |
| `x86_64-pc-windows-gnu`   | Windows, **and cross from Linux/macOS** with MinGW-w64 |

Cross-compiling the GNU target from Linux:

```bash
sudo apt install mingw-w64          # Ubuntu/Debian  (Fedora: mingw64-gcc)
rustup target add x86_64-pc-windows-gnu
./ffi/docker/build-ffi-windows.sh --only x86_64-pc-windows-gnu
```

Targets whose toolchain is missing are **skipped with a message**, never
silently dropped; the run only fails if *nothing* built. The DLL filename is
unversioned (Windows convention); the version lives in the embedded PE
VERSIONINFO resource (see `ffi/build.rs`) and is reported in the summary table
only when Windows tooling is available to read it.

---

## macOS — `build-ffi-macos.sh`

Run on a Mac. Builds `x86_64`, `arm64` and a `lipo`-created `universal2` slice,
then packages `ffi/dist/int2dds-ffi-<ver>-macos.tar.gz`. Minimum OS comes from
`MACOSX_DEPLOYMENT_TARGET` (default `11.0`); `install_name` and the compat /
current versions are embedded by `ffi/build.rs`.

```bash
./ffi/docker/build-ffi-macos.sh
MACOSX_DEPLOYMENT_TARGET=12.0 ./ffi/docker/build-ffi-macos.sh
```

---

## Verifying the artifacts

```bash
# DT_SONAME of every built .so (expects libint2dds_ffi.so.<major>)
./ffi/docker/check-soname.sh
./ffi/docker/check-soname.sh --path ffi/dist/linux-x86_64/libint2dds_ffi.so.0.1.3
```

Static check — the highest GLIBC symbol version the artifact requires:

```bash
objdump -T ffi/dist/linux-x86_64/libint2dds_ffi.so.0.1.3 \
  | grep -oE 'GLIBC_[0-9.]+' | sort -V | tail -1
```

Expect `GLIBC_2.28` or lower. `.github/scripts/ci/stage-native.sh` enforces the
same ceiling in CI and fails the release if it regresses.

Runtime check — actually load it on each target OS. This matches x86_64,
the arch `verify-glibc-floor` in `.github/workflows/release.yml` actually
runs (it builds the loader only for `x86_64-unknown-linux-gnu` and downloads
the `native-linux-x86_64` artifact); on a non-x86_64 host, substitute
`manylinux_2_28_aarch64` and `linux-aarch64` below and register QEMU/binfmt
first (any `build-ffi-linux.sh` run registers them, or see "Docker
prerequisites" above), or the containers fail with an exec-format error:

```bash
docker run --rm -v "$PWD":/w -w /w quay.io/pypa/manylinux_2_28_x86_64 \
  gcc -O2 -o /w/glibc-floor-check .github/scripts/ci/glibc-floor-check.c -ldl
for img in redhat/ubi8 ubuntu:20.04 redhat/ubi9 ubuntu:22.04 ubuntu:24.04; do
  printf '%-16s ' "$img"
  docker run --rm -v "$PWD":/w -w /w "$img" \
    ./glibc-floor-check /w/ffi/dist/linux-x86_64/libint2dds_ffi.so.0.1.3 2>&1 | tail -1
done
rm -f glibc-floor-check
```

Expect `OK  dlopen+call rc=0 has_value=0` on all five. Compile the loader in
the builder image (`manylinux_2_28_*`, which ships gcc), never in a target
container: none of the five target images ships a compiler, and of the five
only `ubi9` ships `python3`. `verify-glibc-floor` runs exactly this against
the x86_64 archive.
