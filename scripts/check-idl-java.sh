#!/usr/bin/env bash
# Fails when the Java backend's output for the IDL corpus does not compile.
# String snapshots cannot catch a syntax error; javac can.
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
  echo "ERROR: the backend produced no Java sources." >&2
  exit 1
fi

echo "Compiling ${#SOURCES[@]} generated sources..."
javac -Xlint:all -d "$OUT/classes" -cp "$API_CLASSES" "${SOURCES[@]}"
echo "Generated Java for the IDL corpus compiles."
