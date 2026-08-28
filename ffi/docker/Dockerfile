# int2dds-ffi Linux builder (multi-arch, glibc 2.28 / AlmaLinux 8)
#
# Builds libint2dds_ffi.so against glibc 2.28 so one artifact per arch runs on
# RHEL 8/9/10 AND Ubuntu 20.04/22.04/24.04. glibc symbol versioning is forward
# compatible, so the lowest builder covers everything above it — there is no
# second axis to build against: this library has no libstdc++ dependency
# (rustls is pinned to the `ring` feature, not aws-lc), so the compiler version
# does not affect artifact compatibility.
#
# quay.io/pypa/manylinux_2_28_* is AlmaLinux 8, which is the RHEL 8 ABI itself,
# and PyPI keeps it maintained. Building our own CentOS-era image would mean
# owning an EOL package repo.
#
# armhf is NOT built from this image — manylinux publishes no armv7 image and
# every older armhf builder (debian 10 archived, debian 11 LTS ending
# 2026-08-31, ubuntu 20.04 EOL) is unmaintained. armhf stays on the ubuntu
# 22.04 cross path in .github/workflows/release.yml, floor 2.35.
#
# The source tree is bind-mounted at run time; only the toolchain lives in the
# image, so code changes never require an image rebuild.

ARG BASE_IMAGE=quay.io/pypa/manylinux_2_28_x86_64
FROM ${BASE_IMAGE}

ARG RUST_VERSION=1.89.0
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

# The manylinux image already ships gcc (via gcc-toolset, ahead of /usr/bin on
# PATH), make, cmake, perl, git, curl, file, pkg-config and binutils — no dnf
# install is needed. Do NOT install plain `gcc` — it would add an older
# /usr/bin/gcc and muddy the toolchain.

# Pin the exact toolchain declared in the workspace (rust-version = 1.89.0).
# rustup auto-detects the image arch, giving a native toolchain per platform.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --no-modify-path --profile minimal --default-toolchain ${RUST_VERSION} \
    && rustc --version && rustc -vV | grep host

WORKDIR /src

# DIST = friendly arch subdir name under ffi/dist (passed in at run time).
# CARGO_TARGET_DIR points at an ephemeral container path so every run is a
# clean build and the host's target/ directory is never touched.
ENV DIST=unknown \
    CARGO_TARGET_DIR=/tmp/cargo-target

# `--locked` is not just hygiene: every target now builds concurrently against
# the same bind-mounted /src, and a cargo run that decides to refresh
# Cargo.lock would have several containers rewriting one host file at once.
# --locked makes that write impossible and fails loudly on a stale lock.
# NOTE: no `pipefail` — diagnostic pipes like `ldd | head` raise SIGPIPE under
# QEMU (head closes the pipe before the slow emulated writer finishes), which
# pipefail would wrongly treat as a build failure (exit 141). `set -eu` still
# aborts on a real cargo build failure since cargo runs as a standalone command.
# Versioned install layout (standard ELF 3-tier, chained like libtool/ldconfig):
#   libint2dds_ffi.so.<major.minor.patch>   real file (full version from Cargo.toml)
#   libint2dds_ffi.so.<major>     -> .so.<major.minor.patch>   soname symlink (matches DT_SONAME)
#   libint2dds_ffi.so             -> .so.<major>               dev/link symlink (points at the soname,
#                                                              not a specific patch, so it rides upgrades)
# The major-only soname is embedded by ffi/build.rs from CARGO_PKG_VERSION_MAJOR.
CMD ["bash", "-lc", "\
    set -eu; \
    echo \"== platform: $(uname -m)  ->  dist/$DIST ==\"; \
    ldd --version | head -1 || true; rustc -vV | grep host || true; \
    cargo build --locked --release -p int2dds-ffi; \
    ver=$(grep -m1 '^version' /src/Cargo.toml | cut -d'\"' -f2); \
    maj=${ver%%.*}; \
    real=libint2dds_ffi.so.$ver; son=libint2dds_ffi.so.$maj; \
    out=/src/ffi/dist/$DIST; mkdir -p \"$out\"; \
    cp -v \"$CARGO_TARGET_DIR/release/libint2dds_ffi.so\" \"$out/$real\"; \
    ln -sfn \"$real\" \"$out/$son\"; \
    ln -sfn \"$son\" \"$out/libint2dds_ffi.so\"; \
    cp -v /src/ffi/include/int2dds-ffi.h /src/ffi/dist/ 2>/dev/null || true; \
    echo '== artifact =='; \
    readelf -h \"$out/$real\" | grep -E 'Class|Machine|Type' || true; \
    echo '== embedded SONAME (should be '$son') =='; \
    readelf -d \"$out/$real\" | grep SONAME || true; \
    echo '== max GLIBC symbol version required (must be <= 2.28) =='; \
    { objdump -T \"$out/$real\" | grep -oE 'GLIBC_[0-9.]+' | sort -V | tail -1; } || true \
    "]
