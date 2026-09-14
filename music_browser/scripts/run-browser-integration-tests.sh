#!/usr/bin/env bash
# Runs `cargo test` with a real WebDriver instance available so
# tests/practice_open_song_file_tests.rs can drive an actual browser
# (issue #65: opening a song's lead sheet link from the Practice page).
#
# What this does:
#   1. Starts the driver for the selected browser (geckodriver for
#      Firefox, the default; safaridriver for Safari) on a free local
#      port.
#   2. Exports WEBDRIVER_URL / WEBDRIVER_BROWSER so the browser test can
#      find it.
#   3. Runs `cargo test` (forwarding any extra args, e.g. a test name filter).
#   4. Always tears down the driver process on exit.
#
# Usage:
#   scripts/run-browser-integration-tests.sh                  # Firefox, headless
#   scripts/run-browser-integration-tests.sh --browser safari # Safari, headed, macOS-only
#   WEBDRIVER_HEADLESS=0 scripts/run-browser-integration-tests.sh  # Firefox, headed
#
# Firefox/geckodriver setup:
#   - Ships preinstalled on GitHub Actions' ubuntu-latest and
#     windows-latest runners (via $GECKOWEBDRIVER).
#   - Locally: `brew install --cask firefox geckodriver` (macOS), or your
#     distro's `firefox`/`firefox-esr` + `geckodriver` packages (Linux).
#
# Safari/safaridriver setup (macOS-only, no headless mode):
#   1. Safari > Settings > Advanced > "Show features for web developers".
#   2. Safari > Develop menu > "Allow Remote Automation".
#   3. `safaridriver --enable` (needs an admin password once).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

BROWSER="${WEBDRIVER_BROWSER:-firefox}"
EXTRA_ARGS=()
while [ $# -gt 0 ]; do
    case "$1" in
        --browser)
            BROWSER="$2"
            shift 2
            ;;
        *)
            EXTRA_ARGS+=("$1")
            shift
            ;;
    esac
done

DRIVER_PORT=$(( (RANDOM % 20000) + 40000 ))
DRIVER_LOG="$(mktemp -t webdriver-log)"
DRIVER_PID=""

cleanup() {
    if [ -n "$DRIVER_PID" ] && kill -0 "$DRIVER_PID" 2>/dev/null; then
        kill "$DRIVER_PID" 2>/dev/null || true
        wait "$DRIVER_PID" 2>/dev/null || true
    fi
    rm -f "$DRIVER_LOG"
}
trap cleanup EXIT INT TERM

case "$BROWSER" in
    firefox)
        # Prefer the runner-provided geckodriver (GitHub Actions'
        # ubuntu-latest / windows-latest images) if present, else PATH.
        if [ -n "${GECKOWEBDRIVER:-}" ] && [ -x "$GECKOWEBDRIVER/geckodriver" ]; then
            GECKODRIVER_BIN="$GECKOWEBDRIVER/geckodriver"
        elif command -v geckodriver >/dev/null 2>&1; then
            GECKODRIVER_BIN="$(command -v geckodriver)"
        else
            echo "error: geckodriver not found. Install Firefox + geckodriver" >&2
            echo "(e.g. 'brew install --cask firefox geckodriver' on macOS)," >&2
            echo "or set GECKOWEBDRIVER to a directory containing geckodriver." >&2
            exit 1
        fi

        echo "==> Starting geckodriver on 127.0.0.1:$DRIVER_PORT"
        "$GECKODRIVER_BIN" --port "$DRIVER_PORT" > "$DRIVER_LOG" 2>&1 &
        DRIVER_PID=$!
        ;;
    safari)
        if ! command -v safaridriver >/dev/null 2>&1; then
            echo "error: safaridriver not found. --browser safari is macOS-only." >&2
            exit 1
        fi

        echo "==> Starting safaridriver on 127.0.0.1:$DRIVER_PORT"
        safaridriver -p "$DRIVER_PORT" > "$DRIVER_LOG" 2>&1 &
        DRIVER_PID=$!
        ;;
    *)
        echo "error: unknown --browser '$BROWSER' (expected 'firefox' or 'safari')" >&2
        exit 1
        ;;
esac

READY=""
for _ in $(seq 1 20); do
    if curl -sf "http://127.0.0.1:$DRIVER_PORT/status" -o /dev/null 2>/dev/null; then
        READY="1"
        break
    fi
    if ! kill -0 "$DRIVER_PID" 2>/dev/null; then
        echo "error: $BROWSER driver exited before becoming ready. Log:" >&2
        cat "$DRIVER_LOG" >&2 || true
        exit 1
    fi
    sleep 0.5
done

if [ -z "$READY" ]; then
    echo "error: $BROWSER driver did not become ready in time. Log:" >&2
    cat "$DRIVER_LOG" >&2 || true
    exit 1
fi

echo "==> Running cargo test ($BROWSER)"
export WEBDRIVER_URL="http://127.0.0.1:$DRIVER_PORT"
export WEBDRIVER_BROWSER="$BROWSER"
cargo test --manifest-path "$SCRIPT_DIR/../Cargo.toml" --test practice_open_song_file_tests \
    "${EXTRA_ARGS[@]+"${EXTRA_ARGS[@]}"}"
