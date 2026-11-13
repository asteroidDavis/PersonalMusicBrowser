#!/usr/bin/env bash
# Run PocketBase over HTTPS with a self-signed local CA, using an Istio
# ingress gateway on a local k3d cluster to terminate TLS.
#
# PocketBase itself has no --cert/--key flags (its --https flag only does
# Let's Encrypt autocert for public domains), so TLS is provided in front of
# it instead:
#
#   client --HTTPS (local CA)--> Istio gateway --Istio mTLS--> PocketBase :8090
#
# Usage: pocketbase-k3d.sh cert|up|down
#
# Requires: docker, k3d, kubectl, istioctl, openssl, curl.
#
# Env overrides:
#   K3D_CLUSTER          (default: music-browser)
#   POCKETBASE_HTTPS_PORT (default: 8090) host port for https://127.0.0.1:<port>
#   POCKETBASE_VERSION   (default: 0.38.0) official release run in the pod
#   ISTIO_PROFILE        (default: default)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PB_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

K3D_CLUSTER="${K3D_CLUSTER:-music-browser}"
POCKETBASE_HTTPS_PORT="${POCKETBASE_HTTPS_PORT:-8090}"
POCKETBASE_VERSION="${POCKETBASE_VERSION:-0.38.0}"
ISTIO_PROFILE="${ISTIO_PROFILE:-default}"

CA_CRT="$PB_DIR/ca.crt"
CA_KEY="$PB_DIR/ca.key"
TLS_CRT="$PB_DIR/localhost.crt"
TLS_KEY="$PB_DIR/localhost.key"

log() { printf '==> %s\n' "$*"; }

require() {
    local missing=0
    for tool in "$@"; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            echo "error: '$tool' not found on PATH" >&2
            missing=1
        fi
    done
    if [ "$missing" -ne 0 ]; then
        echo "Install k3d (https://k3d.io), kubectl and istioctl (https://istio.io/latest/docs/setup/getting-started/#download)." >&2
        exit 1
    fi
}

# Self-signed local CA plus a localhost/127.0.0.1 server cert issued by it.
# Clients trust ca.crt (POCKETBASE_CA_CERT); the gateway serves localhost.crt.
cmd_cert() {
    require openssl
    if [ -f "$CA_CRT" ] && [ -f "$TLS_CRT" ] && [ -f "$TLS_KEY" ] \
        && openssl verify -CAfile "$CA_CRT" "$TLS_CRT" >/dev/null 2>&1; then
        log "Reusing existing certificates in $PB_DIR"
        return
    fi

    log "Generating self-signed local CA: $CA_CRT"
    openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 365 \
        -keyout "$CA_KEY" -out "$CA_CRT" \
        -subj "/CN=PersonalMusicBrowser Local CA" \
        -addext "basicConstraints=critical,CA:TRUE" \
        -addext "keyUsage=critical,keyCertSign,cRLSign" 2>/dev/null

    log "Issuing PocketBase server certificate: $TLS_CRT"
    local tmp
    tmp="$(mktemp -d)"
    openssl req -newkey rsa:2048 -nodes -sha256 \
        -keyout "$TLS_KEY" -out "$tmp/server.csr" -subj "/CN=localhost" 2>/dev/null
    cat >"$tmp/server.ext" <<EXT
basicConstraints=CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=DNS:localhost,IP:127.0.0.1
EXT
    openssl x509 -req -sha256 -days 365 -in "$tmp/server.csr" \
        -CA "$CA_CRT" -CAkey "$CA_KEY" -CAcreateserial -CAserial "$tmp/ca.srl" \
        -extfile "$tmp/server.ext" -out "$TLS_CRT" 2>/dev/null
    rm -rf "$tmp"
    chmod 600 "$CA_KEY" "$TLS_KEY"
}

cmd_up() {
    require docker k3d kubectl istioctl openssl curl
    cmd_cert

    mkdir -p "$PB_DIR/pb_data" "$PB_DIR/pb_migrations"

    if k3d cluster get "$K3D_CLUSTER" >/dev/null 2>&1; then
        log "Using existing k3d cluster '$K3D_CLUSTER'"
        k3d cluster start "$K3D_CLUSTER" >/dev/null
    else
        log "Creating k3d cluster '$K3D_CLUSTER' (https://127.0.0.1:$POCKETBASE_HTTPS_PORT -> gateway :443)"
        k3d cluster create "$K3D_CLUSTER" \
            --k3s-arg "--disable=traefik@server:0" \
            -p "127.0.0.1:${POCKETBASE_HTTPS_PORT}:443@loadbalancer" \
            --volume "$PB_DIR/pb_data:/pocketbase/pb_data@server:0" \
            --volume "$PB_DIR/pb_migrations:/pocketbase/pb_migrations@server:0" \
            --wait
    fi
    local ctx="k3d-$K3D_CLUSTER"
    local kc=(kubectl --context "$ctx")

    if "${kc[@]}" -n istio-system get deploy istiod istio-ingressgateway >/dev/null 2>&1; then
        log "Istio already installed"
    else
        log "Installing Istio (profile=$ISTIO_PROFILE)"
        istioctl --context "$ctx" install -y --set profile="$ISTIO_PROFILE"
    fi

    log "Loading TLS secret istio-system/pocketbase-tls"
    "${kc[@]}" -n istio-system create secret tls pocketbase-tls \
        --cert "$TLS_CRT" --key "$TLS_KEY" --dry-run=client -o yaml \
        | "${kc[@]}" apply -f -

    log "Deploying PocketBase v$POCKETBASE_VERSION"
    sed -e "s/\${POCKETBASE_VERSION}/$POCKETBASE_VERSION/g" \
        -e "s/\${PB_UID}/$(id -u)/g" \
        -e "s/\${PB_GID}/$(id -g)/g" \
        "$SCRIPT_DIR/pocketbase.yaml" | "${kc[@]}" apply -f -
    "${kc[@]}" -n pocketbase rollout status deploy/pocketbase --timeout=300s

    local url="https://127.0.0.1:$POCKETBASE_HTTPS_PORT"
    log "Waiting for $url/api/health (verified against $CA_CRT)"
    for _ in $(seq 1 60); do
        if curl -fsS --cacert "$CA_CRT" "$url/api/health" >/dev/null 2>&1; then
            curl -fsS --cacert "$CA_CRT" "$url/api/health"
            echo
            cat <<MSG

PocketBase is serving HTTPS at $url (admin UI: $url/_/)
Create a superuser with:
  kubectl --context $ctx -n pocketbase exec deploy/pocketbase -c pocketbase -- \\
    /opt/pb/pocketbase superuser upsert --dir /pocketbase/pb_data <email> <password>

Set in music_browser/.env (or run \`make rust-run-secure\`):
  POCKETBASE_URL=$url
  POCKETBASE_CA_CERT=../pocketbase/ca.crt
  AUTH_COOKIE_SECURE=true
MSG
            return
        fi
        sleep 2
    done
    echo "error: $url/api/health did not become healthy" >&2
    exit 1
}

cmd_down() {
    require k3d
    if k3d cluster get "$K3D_CLUSTER" >/dev/null 2>&1; then
        log "Stopping k3d cluster '$K3D_CLUSTER' (data stays in $PB_DIR/pb_data)"
        k3d cluster stop "$K3D_CLUSTER"
    else
        log "No k3d cluster '$K3D_CLUSTER'"
    fi
}

case "${1:-}" in
    cert) cmd_cert ;;
    up) cmd_up ;;
    down) cmd_down ;;
    *)
        echo "usage: $0 cert|up|down" >&2
        exit 2
        ;;
esac
