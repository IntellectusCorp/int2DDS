# FFI native builds (Linux / Windows / macOS)

Build scripts that produce the distributable `int2dds-ffi` native libraries.
Every platform has a **`.sh` (bash)** entry point; the Linux and Windows ones
also keep their original **`.ps1` (PowerShell)** twin. The pairs are feature-
equivalent — same targets, same `ffi/dist` layout, same manifest fields — so
pick whichever matches the shell you are in.

| Platform     | bash                   | PowerShell              | Needs                 |
| ------------ | ---------------------- | ----------------------- | --------------------- |
| Linux        | `build-ffi-linux.sh`   | `build-ffi-linux.ps1`   | Docker                |
| Windows      | `build-ffi-windows.sh` | `build-ffi-windows.ps1` | cargo + toolchain     |
| macOS        | `build-ffi-macos.sh`   | —                       | cargo + Xcode CLT     |
| SONAME check | `check-soname.sh`      | `check-soname.ps1`      | nothing (or binutils) |

---

## Linux — `build-ffi-linux.sh`

Clean-builds `libint2dds_ffi.so` for every Linux architecture the RMW layer
needs, in one command. glibc targets are built against **glibc 2.35**
(Ubuntu 22.04); musl targets against **Alpine 3.20**.

```bash
./ffi/docker/build-ffi-linux.sh                      # all 5 targets + tarball
./ffi/docker/build-ffi-linux.sh --only linux/arm64   # gnu + musl arm64
./ffi/docker/build-ffi-linux.sh --only linux-x86_64  # a single dist dir
./ffi/docker/build-ffi-linux.sh --no-package         # build only, no tarball
```

| Flag                 | Effect                                                                    |
| -------------------- | ------------------------------------------------------------------------- |
| `--only <list>`      | Subset by docker platform (`linux/amd64`) **or** dist dir (`linux-armhf`) |
| `--rust-version <x>` | Override the toolchain (default `1.89.0`)                                 |
| `--no-package`       | Skip the `.tar.gz` distribution archive                                   |
| `--no-binfmt`        | Skip the privileged QEMU binfmt registration container                    |

### Why Ubuntu 22.04

glibc is forward-compatible: a binary linked against an older glibc runs on
newer ones, but not vice-versa. Building on 22.04 (glibc 2.35) yields a single
`.so` per arch that runs on **both Ubuntu 22.04 and 24.04**, so we ship one
shared artifact per arch instead of one per distro. (In practice the libraries
only require up to `GLIBC_2.34`.)

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
  run under QEMU emulation. This keeps the aws-lc-sys / ring crypto crates
  (cmake + C/asm) building exactly as on real hardware, which is more reliable
  than cross-linking.
- The repo is bind-mounted at `/src`; only the toolchain lives in the image, so
  code changes need no image rebuild.
- `CARGO_TARGET_DIR` is an ephemeral container path, so the host's `target/`
  directory is never touched and every run is clean.
- After each compile the container prints the **highest required GLIBC symbol
  version** — confirm it is `<= GLIBC_2.35` to guarantee 24.04 compatibility.
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

### Gotchas handled by this setup

- **arm64/armhf `libc-bin` segfault (exit 139)** during apt — a stale bundled
  QEMU; the script re-registers up-to-date emulators via `tonistiigi/binfmt`
  each run (registration is not persistent across daemon restarts).
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
and pass `--no-binfmt`.

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
./ffi/docker/check-soname.sh --path ffi/dist/linux-x86_64/libint2dds_ffi.so.0.1.1

# highest required glibc symbol version
objdump -T ffi/dist/linux-aarch64/libint2dds_ffi.so \
  | grep -oE 'GLIBC_[0-9.]+' | sort -V | tail -1
```
