# Linux FFI build (multi-arch, Ubuntu 22.04)

Clean-builds `libint2dds_ffi.so` for every Linux architecture the RMW layer
needs, against **glibc 2.35** (Ubuntu 22.04), in one command.

## Why 22.04

glibc is forward-compatible: a binary linked against an older glibc runs on
newer ones, but not vice-versa. Building on 22.04 (glibc 2.35) yields a single
`.so` per arch that runs on **both Ubuntu 22.04 and 24.04**, so we ship one
shared artifact per arch instead of one per distro. (In practice the libraries
only require up to `GLIBC_2.34`.)

## Usage (Windows / PowerShell)

```powershell
.\ffi\docker\build-ffi-linux.ps1
```

Every run is a **clean build**: `ffi/dist` is wiped first and the in-container
Cargo target dir is ephemeral.

### Output layout

```
ffi/dist/
  int2dds-ffi.h                       # architecture-independent C header
  linux-x86_64/libint2dds_ffi.so      # ELF64 x86-64
  linux-aarch64/libint2dds_ffi.so     # ELF64 AArch64 (arm64)
  linux-armhf/libint2dds_ffi.so       # ELF32 ARM (32-bit, armv7 gnueabihf)
```

`ffi/dist/` is git-ignored.

### Options

| Flag                  | Effect                                                      |
| --------------------- | ---------------------------------------------------------- |
| `-Only <platforms>`   | Subset, e.g. `-Only linux/amd64,linux/arm64`               |
| `-RustVersion <x>`    | Override the toolchain (default `1.89.0`)                  |

## How it works

- Each architecture is compiled **natively inside its own-arch container**
  (`docker build/run --platform ...`): amd64 runs natively, arm64/armhf run
  under Docker Desktop's QEMU emulation. This keeps the aws-lc-sys / ring
  crypto crates (cmake + C/asm) building exactly as on real hardware, which is
  more reliable than cross-linking.
- The repo is bind-mounted at `/src`; only the toolchain lives in the image, so
  code changes need no image rebuild.
- `CARGO_TARGET_DIR` is an ephemeral container path, so the host's Windows
  `target\` directory is never touched and every run is clean.
- After each compile the container prints the **highest required GLIBC symbol
  version** — confirm it is `<= GLIBC_2.35` to guarantee 24.04 compatibility.

## Build time

Emulated builds are slow. Expect roughly:

| Target | Mode        | Approx compile time |
| ------ | ----------- | ------------------- |
| amd64  | native      | ~3 min              |
| arm64  | QEMU        | ~15 min             |
| armhf  | QEMU        | ~23 min             |

## Gotchas handled by this setup

- **arm64/armhf `libc-bin` segfault (exit 139)** during apt — Docker Desktop's
  bundled QEMU is too old; the script re-registers up-to-date emulators via
  `tonistiigi/binfmt` each run (registration is not persistent).
- **SIGPIPE (exit 141)** under emulation — diagnostic pipes like `ldd | head`
  raise SIGPIPE when the reader closes early; the container script avoids
  `pipefail` so these never abort the build.
- **32-bit `off_t`** — `ftruncate` takes `i32` on armhf vs `i64` on 64-bit;
  fixed in `dds/src/rtps/transport/shm/platform/unix.rs` by casting to
  `libc::off_t` instead of a fixed `i64`.

## Verifying compatibility manually

```bash
objdump -T ffi/dist/linux-aarch64/libint2dds_ffi.so \
  | grep -oE 'GLIBC_[0-9.]+' | sort -V | tail -1
```
