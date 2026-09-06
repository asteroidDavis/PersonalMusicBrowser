#!/usr/bin/env bash
# Runs `cargo test` with a real, ephemeral PocketBase instance available so
# tests/pocketbase_client_integration_tests.rs can exercise the shares/groups
# ACL collections against an actual server instead of mocks.
#
# What this does:
#   1. Creates a throwaway PocketBase data directory (mktemp -d).
#   2. Applies pocketbase/pb_migrations (including the CI-only test-user seed
#      migration, gated behind PB_TEST_SEED=true) into that fresh directory.
#   3. Starts `pocketbase serve` on a free local port against that directory.
#   4. Exports POCKETBASE_TEST_URL so the integration tests can find it.
#   5. Runs `cargo test` (forwarding any extra args, e.g. a test name filter).
#   6. Always tears down the PocketBase process and temp directory on exit.
#
# Used by CI (.github/workflows/ci.yml) and the local pre-commit hook
# (.pre-commit-config.yaml / scripts/install-hooks.sh). Requires a
# `pocketbase` binary either at pocketbase/pocketbase (repo-relative) or on
# PATH; override with POCKETBASE_BIN.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
MIGRATIONS_DIR="$REPO_ROOT/pocketbase/pb_migrations"

if [ -n "${POCKETBASE_BIN:-}" ]; then
    PB_BIN="$POCKETBASE_BIN"
elif [ -x "$REPO_ROOT/pocketbase/pocketbase" ]; then
    PB_BIN="$REPO_ROOT/pocketbase/pocketbase"
elif command -v pocketbase >/dev/null 2>&1; then
    PB_BIN="$(command -v pocketbase)"
else
    echo "error: no pocketbase binary found. Set POCKETBASE_BIN, place a" >&2
    echo "binary at pocketbase/pocketbase, or install it on PATH." >&2
    exit 1
fi

PB_DATA_DIR="$(mktemp -d)"
PB_PID=""

cleanup() {
    if [ -n "$PB_PID" ] && kill -0 "$PB_PID" 2>/dev/null; then
        kill "$PB_PID" 2>/dev/null || true
        wait "$PB_PID" 2>/dev/null || true
    fi
    rm -rf "$PB_DATA_DIR"
}
trap cleanup EXIT INT TERM

# Pick a (likely) free local port so repeated/parallel runs don't collide.
PB_PORT=$(( (RANDOM % 20000) + 20000 ))

echo "==> Applying PocketBase migrations to $PB_DATA_DIR"
PB_TEST_SEED=true "$PB_BIN" migrate up \
    --dir "$PB_DATA_DIR" \
    --migrationsDir "$MIGRATIONS_DIR"

echo "==> Starting PocketBase on 127.0.0.1:$PB_PORT"
PB_TEST_SEED=true "$PB_BIN" serve \
    --dir "$PB_DATA_DIR" \
    --migrationsDir "$MIGRATIONS_DIR" \
    --http "127.0.0.1:$PB_PORT" \
    > "$PB_DATA_DIR/pocketbase.log" 2>&1 &
PB_PID=$!

READY=""
for _ in $(seq 1 60); do
    if curl -sf "http://127.0.0.1:$PB_PORT/api/health" >/dev/null 2>&1; then
        READY="1"
        break
    fi
    if ! kill -0 "$PB_PID" 2>/dev/null; then
        echo "error: pocketbase exited before becoming ready. Log:" >&2
        cat "$PB_DATA_DIR/pocketbase.log" >&2 || true
        exit 1
    fi
    sleep 0.5
done

if [ -z "$READY" ]; then
    echo "error: pocketbase did not become ready in time. Log:" >&2
    cat "$PB_DATA_DIR/pocketbase.log" >&2 || true
    exit 1
fi

echo "==> Running cargo test"
export POCKETBASE_TEST_URL="http://127.0.0.1:$PB_PORT"
cargo test --manifest-path "$REPO_ROOT/music_browser/Cargo.toml" "$@"
