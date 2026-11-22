#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFLIGHT="$SCRIPT_DIR/preflight.sh"

extract_summary() {
    awk '/^-----BEGIN SUMMARY-----$/{f=1; next} /^-----END SUMMARY-----$/{f=0} f'
}

assert_kv() {
    local summary="$1" expected="$2" label="$3" out="$4"
    if ! printf '%s\n' "$summary" | grep -Fxq "$expected"; then
        printf 'FAIL [%s]: expected %s in summary\n' "$label" "$expected" >&2
        printf '%s\n' "$out" >&2
        exit 1
    fi
}

assert_absent_key() {
    local summary="$1" key="$2" label="$3" out="$4"
    if printf '%s\n' "$summary" | grep -Eq "^${key}="; then
        printf 'FAIL [%s]: expected key %s to be absent in summary\n' "$label" "$key" >&2
        printf '%s\n' "$out" >&2
        exit 1
    fi
}

run_unready() {
    echo "==> Scenario: unready container (alpine:3.20, no Docker)"
    local out summary
    out="$(docker run --rm -i alpine:3.20 sh -s -- --container-tests < "$PREFLIGHT")"
    summary="$(printf '%s\n' "$out" | extract_summary)"
    assert_kv "$summary" "VERDICT_K3D=no-go" "unready" "$out"
    assert_kv "$summary" "VERDICT_COMPOSE=no-go" "unready" "$out"
    assert_kv "$summary" "VERDICT_PREINSTALLED_K3S=not-found" "unready" "$out"
    assert_absent_key "$summary" "CONTAINER_RUN" "unready" "$out"
    assert_absent_key "$summary" "PRIVILEGED" "unready" "$out"
    echo "    ok"
}

run_preinstalled_k3s() {
    echo "==> Scenario: preinstalled k3s container (rancher/k3s, no Docker)"
    local out summary
    out="$(docker run --rm -i --entrypoint /bin/sh rancher/k3s:v1.31.0-k3s1 -s < "$PREFLIGHT")"
    summary="$(printf '%s\n' "$out" | extract_summary)"
    assert_kv "$summary" "VERDICT_K3D=no-go" "preinstalled-k3s" "$out"
    assert_kv "$summary" "VERDICT_COMPOSE=no-go" "preinstalled-k3s" "$out"
    assert_kv "$summary" "VERDICT_PREINSTALLED_K3S=available" "preinstalled-k3s" "$out"
    echo "    ok"
}

run_ready() {
    echo "==> Scenario: ready container with Docker daemon (without --container-tests)"
    local out summary
    out="$(docker run --rm -i --cgroupns=host -v /var/run/docker.sock:/var/run/docker.sock \
        docker:27-cli sh -s < "$PREFLIGHT")"
    summary="$(printf '%s\n' "$out" | extract_summary)"
    assert_kv "$summary" "VERDICT_K3D=likely" "ready-default" "$out"
    assert_kv "$summary" "VERDICT_COMPOSE=go" "ready-default" "$out"
    assert_kv "$summary" "VERDICT_PREINSTALLED_K3S=not-found" "ready-default" "$out"
    assert_absent_key "$summary" "CONTAINER_RUN" "ready-default" "$out"
    assert_absent_key "$summary" "PRIVILEGED" "ready-default" "$out"
    echo "    ok"

    echo "==> Scenario: ready container with Docker daemon (--container-tests)"
    out="$(docker run --rm -i --cgroupns=host -v /var/run/docker.sock:/var/run/docker.sock \
        docker:27-cli sh -s -- --container-tests < "$PREFLIGHT")"
    summary="$(printf '%s\n' "$out" | extract_summary)"
    assert_kv "$summary" "CONTAINER_RUN=ok" "ready-container-tests" "$out"
    assert_kv "$summary" "PRIVILEGED=ok" "ready-container-tests" "$out"
    assert_kv "$summary" "VERDICT_K3D=go" "ready-container-tests" "$out"
    assert_kv "$summary" "VERDICT_COMPOSE=go" "ready-container-tests" "$out"
    assert_kv "$summary" "VERDICT_PREINSTALLED_K3S=not-found" "ready-container-tests" "$out"
    echo "    ok"
}

case "${1:-all}" in
    unready) run_unready ;;
    preinstalled-k3s) run_preinstalled_k3s ;;
    ready) run_ready ;;
    all)
        run_unready
        run_preinstalled_k3s
        run_ready
        ;;
    *)
        echo "usage: $0 [unready|preinstalled-k3s|ready|all]" >&2
        exit 2
        ;;
esac
