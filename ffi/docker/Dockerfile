# int2dds-ffi Linux builder (multi-arch, glibc 2.35 / Ubuntu 22.04)
#
# Builds libint2dds_ffi.so on Ubuntu 22.04 so the artifact links against
# glibc 2.35 and is forward-compatible (runs on 22.04 AND 24.04).
#
# This image is built once PER PLATFORM (linux/amd64, linux/arm64,
# linux/arm/v7) via `docker build --platform ...`. Inside each platform's
# image rustup installs that platform's NATIVE toolchain, so aws-lc-sys / ring
# (which need cmake + a C/asm toolchain) build exactly as on real hardware.
# arm64 / armhf images run under QEMU emulation provided by Docker Desktop.
#
# The source tree is bind-mounted at run time; only the toolchain lives in the
# image, so code changes never require an image rebuild.

FROM ubuntu:22.04

ARG RUST_VERSION=1.89.0
ENV DEBIAN_FRONTEND=noninteractive \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# build-essential        -> native gcc/ld for the cdylib link step
# cmake / perl           -> required to build aws-lc-sys and ring from source
# clang / libclang-dev   -> aws-lc-sys bindgen fallback
# pkg-config             -> common -sys crate discovery
# binutils (objdump/readelf) ships with build-essential for the GLIBC check
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        cmake \
        perl \
        clang \
        libclang-dev \
        curl \
        ca-certificates \
        pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Pin the exact toolchain declared in the workspace (rust-version = 1.89.0).
# rustup auto-detects the image arch, giving a native toolchain per platform.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain ${RUST_VERSION} \
    && rustc --version && rustc -vV | grep host

WORKDIR /src

# DIST = friendly arch subdir name under ffi/dist (passed in at run time).
# CARGO_TARGET_DIR points at an ephemeral container path so every run is a
# clean build and the host's Windows target/ directory is never touched.
# This image builds the glibc (gnu) targets only. musl targets use the separate
# Alpine-based Dockerfile.musl (Alpine is musl-native, so libgcc_s is available
# for the dynamic cdylib link — unlike ubuntu's musl-tools).
ENV DIST=unknown \
    CARGO_TARGET_DIR=/tmp/cargo-target

# NOTE: no `pipefail` — diagnostic pipes like `ldd | head` raise SIGPIPE under
# QEMU (head closes the pipe before the slow emulated writer finishes), which
# pipefail would wrongly treat as a build failure (exit 141). `set -eu` still
# aborts on a real cargo build failure since cargo runs as a standalone command.
# Versioned install layout (standard ELF 3-tier):
#   libint2dds_ffi.so.<major.minor.patch>   real file (full version from Cargo.toml)
#   libint2dds_ffi.so.<major.minor>         soname symlink (matches DT_SONAME set by build.rs)
#   libint2dds_ffi.so                       dev/link symlink
# The major.minor soname is embedded by ffi/build.rs from CARGO_PKG_VERSION_*.
CMD ["bash", "-lc", "\
    set -eu; \
    echo \"== platform: $(uname -m)  ->  dist/$DIST ==\"; \
    ldd --version | head -1 || true; rustc -vV | grep host || true; \
    cargo build --release -p int2dds-ffi; \
    ver=$(grep -m1 '^version' /src/Cargo.toml | cut -d'\"' -f2); \
    mm=${ver%.*}; \
    real=libint2dds_ffi.so.$ver; son=libint2dds_ffi.so.$mm; \
    out=/src/ffi/dist/$DIST; mkdir -p \"$out\"; \
    cp -v \"$CARGO_TARGET_DIR/release/libint2dds_ffi.so\" \"$out/$real\"; \
    ln -sfn \"$real\" \"$out/$son\"; \
    ln -sfn \"$real\" \"$out/libint2dds_ffi.so\"; \
    cp -v /src/ffi/include/int2dds-ffi.h /src/ffi/dist/ 2>/dev/null || true; \
    echo '== artifact =='; \
    readelf -h \"$out/$real\" | grep -E 'Class|Machine|Type' || true; \
    echo '== embedded SONAME (should be '$son') =='; \
    readelf -d \"$out/$real\" | grep SONAME || true; \
    echo '== max GLIBC symbol version required (must be <= 2.35 for 24.04 compat) =='; \
    { objdump -T \"$out/$real\" | grep -oE 'GLIBC_[0-9.]+' | sort -V | tail -1; } || true \
    "]
