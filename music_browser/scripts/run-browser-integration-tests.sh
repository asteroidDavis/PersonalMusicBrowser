#!/usr/bin/env bash
# Runs `cargo test` with a real `safaridriver` instance available so
# tests/practice_open_song_file_tests.rs can drive an actual browser
# (issue #65: opening a song's lead sheet link from the Practice page).
#
# What this does:
#   1. Starts `safaridriver` on a free local port.
#   2. Exports SAFARIDRIVER_URL so the browser test can find it.
#   3. Runs `cargo test` (forwarding any extra args, e.g. a test name filter).
#   4. Always tears down the safaridriver process on exit.
#
# macOS-only. One-time setup required before this will work:
#   1. Safari > Settings > Advanced > "Show features for web developers".
#   2. Safari > Develop menu > "Allow Remote Automation".
#   3. `safaridriver --enable` (needs an admin password once).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if ! command -v safaridriver >/dev/null 2>&1; then
    echo "error: safaridriver not found. This script is macOS-only." >&2
    exit 1
fi

SAFARIDRIVER_PORT=$(( (RANDOM % 20000) + 40000 ))
SAFARIDRIVER_LOG="$(mktemp -t safaridriver-log)"
SAFARIDRIVER_PID=""

cleanup() {
    if [ -n "$SAFARIDRIVER_PID" ] && kill -0 "$SAFARIDRIVER_PID" 2>/dev/null; then
        kill "$SAFARIDRIVER_PID" 2>/dev/null || true
        wait "$SAFARIDRIVER_PID" 2>/dev/null || true
    fi
    rm -f "$SAFARIDRIVER_LOG"
}
trap cleanup EXIT INT TERM

echo "==> Starting safaridriver on 127.0.0.1:$SAFARIDRIVER_PORT"
safaridriver -p "$SAFARIDRIVER_PORT" > "$SAFARIDRIVER_LOG" 2>&1 &
SAFARIDRIVER_PID=$!

READY=""
for _ in $(seq 1 20); do
    if curl -sf "http://127.0.0.1:$SAFARIDRIVER_PORT/status" -o /dev/null 2>/dev/null; then
        READY="1"
        break
    fi
    if ! kill -0 "$SAFARIDRIVER_PID" 2>/dev/null; then
        echo "error: safaridriver exited before becoming ready. Log:" >&2
        cat "$SAFARIDRIVER_LOG" >&2 || true
        exit 1
    fi
    sleep 0.5
done

if [ -z "$READY" ]; then
    echo "error: safaridriver did not become ready in time (is 'Allow Remote" >&2
    echo "Automation' enabled in Safari's Develop menu, and has" >&2
    echo "'safaridriver --enable' been run)? Log:" >&2
    cat "$SAFARIDRIVER_LOG" >&2 || true
    exit 1
fi

echo "==> Running cargo test"
export SAFARIDRIVER_URL="http://127.0.0.1:$SAFARIDRIVER_PORT"
cargo test --manifest-path "$SCRIPT_DIR/../Cargo.toml" --test practice_open_song_file_tests "$@"
