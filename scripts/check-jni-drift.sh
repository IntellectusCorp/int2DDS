#!/usr/bin/env bash
# Fails when the committed JNI layer differs from what the generator produces.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo run --quiet -p int2dds-java --bin gen-jni
if ! git diff --quiet -- \
    java/native/src/generated.rs \
    java/api/src/main/java/com/intellectus/int2dds/internal/ffi/Ffi.java; then
  echo "ERROR: generated JNI layer is stale." >&2
  echo "Run: cargo run -p int2dds-java --bin gen-jni" >&2
  git --no-pager diff --stat -- \
    java/native/src/generated.rs \
    java/api/src/main/java/com/intellectus/int2dds/internal/ffi/Ffi.java >&2
  exit 1
fi
echo "JNI layer is up to date."
