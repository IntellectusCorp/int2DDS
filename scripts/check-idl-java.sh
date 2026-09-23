#!/usr/bin/env bash
# Two phases:
#   1. the Java backend's output for the whole IDL corpus compiles
#      (string snapshots cannot catch a syntax error; javac can), and
#   2. the committed CdrGolden.java still matches what the generator emits.
set -euo pipefail
cd "$(dirname "$0")/.."

API_CLASSES="java/api/build/classes/java/main"
if [ ! -d "$API_CLASSES" ]; then
  echo "Building the Java API classes first..." >&2
  (cd java && ./gradlew --quiet :api:classes)
fi

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT

cargo build --quiet -p int2dds-idl
BIN="target/debug/int2dds-idl"

for idl in idl/input/*.idl; do
  "$BIN" "$idl" -j "$OUT/src" --java-package generated.$(basename "$idl" .idl | tr 'A-Z_' 'a-z.') \
      >/dev/null
done

mapfile -t SOURCES < <(find "$OUT/src" -name '*.java')
if [ "${#SOURCES[@]}" -eq 0 ]; then
  echo "ERROR: the IDL corpus produced no Java sources." >&2
  exit 1
fi

echo "Compiling ${#SOURCES[@]} generated sources..."
if ! javac -Xlint:all -d "$OUT/classes" -cp "$API_CLASSES" "${SOURCES[@]}"; then
  echo "ERROR: generated Java for the IDL corpus does not compile." >&2
  exit 1
fi
echo "Phase 1 OK: generated Java for the IDL corpus compiles."

# Phase 2. CdrGolden.java is generated but committed, because
# GeneratedTypeConformanceTest hands its bytes to the core. Regenerate in
# place and let git decide whether the committed copy is still current.
GOLDEN_IDL="idl/input/CdrGolden.idl"
GOLDEN_JAVA="java/api/src/test/java/com/intellectus/int2dds/types/CdrGolden.java"
GOLDEN_CMD="./target/debug/int2dds-idl $GOLDEN_IDL -j java/api/src/test/java \
--java-package com.intellectus.int2dds.types"

# An untracked file has an empty `git diff`, which would pass vacuously.
if ! git ls-files --error-unmatch "$GOLDEN_JAVA" >/dev/null 2>&1; then
  echo "ERROR: $GOLDEN_JAVA is not tracked; the drift check cannot see it." >&2
  exit 1
fi

"$BIN" "$GOLDEN_IDL" -j java/api/src/test/java \
    --java-package com.intellectus.int2dds.types >/dev/null

if ! git diff --quiet -- "$GOLDEN_JAVA"; then
  echo "ERROR: the committed $GOLDEN_JAVA is stale." >&2
  echo "Run: $GOLDEN_CMD" >&2
  git --no-pager diff -- "$GOLDEN_JAVA" >&2
  exit 1
fi
echo "Phase 2 OK: the committed CdrGolden.java matches the generator."
