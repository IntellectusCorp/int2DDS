#!/usr/bin/env bash
#
# Run one hello_world publisher/subscriber pair and report whether the subscriber
# received a sample carrying the publisher's language tag.
#
#   hello_world_pair.sh <pub-lang> <sub-lang> <domain-id> <log-dir>
#
# Languages: rust | c | python | csharp. Exits 0 on a received sample, 1 otherwise.

set -uo pipefail

PUB_LANG="$1"
SUB_LANG="$2"
DOMAIN="$3"
LOG_DIR="$4"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEADLINE_SECS="${HW_PAIR_TIMEOUT:-40}"
DOTNET_TFM="${HW_DOTNET_TFM:-net8.0}"

mkdir -p "$LOG_DIR"
PUB_LOG="$LOG_DIR/${PUB_LANG}_to_${SUB_LANG}.pub.log"
SUB_LOG="$LOG_DIR/${PUB_LANG}_to_${SUB_LANG}.sub.log"

export LD_LIBRARY_PATH="$ROOT/target/debug${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export INT2DDS_UDP_SOCKET_BUFFER="${INT2DDS_UDP_SOCKET_BUFFER:-5242880}"

lang_tag() {
    case "$1" in
        rust)   printf 'Rust' ;;
        c)      printf 'C' ;;
        python) printf 'Python' ;;
        csharp) printf 'C#' ;;
        *)      return 1 ;;
    esac
}

# Fills the CMD array with the argv for <lang> in <role> (pub|sub).
build_cmd() {
    local lang="$1" role="$2" proj
    case "$lang" in
        rust)
            CMD=("$ROOT/target/debug/examples/hello_world_$role")
            ;;
        c)
            CMD=("$ROOT/ffi/examples/build/hello_world_$role")
            ;;
        python)
            # INT2DDS_FFI_PATH is a file for Python, and is set explicitly because the
            # binding's search order prefers target/release over target/debug.
            CMD=(env "INT2DDS_FFI_PATH=$ROOT/target/debug/libint2dds_ffi.so"
                 PYTHONUNBUFFERED=1
                 python3 -u "$ROOT/python/examples/hello_world_$role.py")
            ;;
        csharp)
            if [ "$role" = pub ]; then proj=HelloWorldPub; else proj=HelloWorldSub; fi
            CMD=(dotnet "$ROOT/csharp/examples/$proj/bin/Release/$DOTNET_TFM/$proj.dll")
            ;;
        *)
            echo "unknown language: $lang" >&2
            return 1
            ;;
    esac
}

# SIGINT first — every example installs a handler for it — and SIGKILL only if it hangs.
stop_proc() {
    local pid="$1" i
    kill -0 "$pid" 2>/dev/null || return 0
    kill -INT "$pid" 2>/dev/null
    for i in $(seq 1 20); do
        kill -0 "$pid" 2>/dev/null || return 0
        sleep 0.5
    done
    kill -KILL "$pid" 2>/dev/null
}

TAG="$(lang_tag "$PUB_LANG")" || { echo "unknown language: $PUB_LANG" >&2; exit 1; }
PAYLOAD="[$TAG]HelloWorld_d$DOMAIN"

build_cmd "$SUB_LANG" sub || exit 1
"${CMD[@]}" -d "$DOMAIN" --reliable >"$SUB_LOG" 2>&1 &
SUB_PID=$!

# Let the reader announce itself before the writer starts discovery.
sleep 2

build_cmd "$PUB_LANG" pub || exit 1
"${CMD[@]}" -d "$DOMAIN" --reliable >"$PUB_LOG" 2>&1 &
PUB_PID=$!

# The index is deliberately not pinned: only the payload has to match.
status=1
for _ in $(seq 1 $((DEADLINE_SECS * 2))); do
    if grep -qF "$PAYLOAD" "$SUB_LOG" 2>/dev/null; then
        status=0
        break
    fi
    kill -0 "$PUB_PID" 2>/dev/null || kill -0 "$SUB_PID" 2>/dev/null || break
    sleep 0.5
done

stop_proc "$PUB_PID"
stop_proc "$SUB_PID"
wait "$PUB_PID" 2>/dev/null
wait "$SUB_PID" 2>/dev/null

if [ "$status" -eq 0 ]; then
    echo "PASS  $PUB_LANG -> $SUB_LANG (domain $DOMAIN)"
else
    echo "FAIL  $PUB_LANG -> $SUB_LANG (domain $DOMAIN), expected payload $PAYLOAD"
    echo "--- publisher log ---"
    cat "$PUB_LOG"
    echo "--- subscriber log ---"
    cat "$SUB_LOG"
fi

exit "$status"
