#!/usr/bin/env bash
# Deploy the music browser to a small LAN host ("mini") running Docker.
# Runs on your laptop; the host only needs Docker (+ k3d, which `tools`
# installs). kubectl and helm run locally through an SSH tunnel.
#
#   deploy/mini/mini.sh <command> [args]     (HOST=admin@<ip>, or HOST=local)
#
#   tools                 install pinned k3d on the host
#   up                    create the k3d cluster (k3s, no traefik, ports on LAN_IP)
#   images                build images for the host arch here and load them there
#   istio                 install Istio (low-memory values)
#   deploy [helm args]    helm upgrade --install the chart (mini + arch values)
#   status                pods, host memory, container memory
#   backup                stop app + PocketBase, tar their data, start again
#   restore <backup.tgz>  stop, replace data with the backup, start
#   upgrade <tag> [args]  backup, then deploy IMAGE_TAG=<tag>
#   rollback              helm rollback to the previous release
#   disk-bench            fsync latency of the data dir (decides on SSD cache)
#   tunnel                (re)open the kube API tunnel
#   down                  delete the cluster (data and backups are kept)
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
HOST="${HOST:-}"
MINI_ROOT="${MINI_ROOT:-~/music-browser}"   # persistent dir on the host
DOCKER_DIR="${DOCKER_DIR:-}"                # dir holding docker if not on PATH
CLUSTER="${CLUSTER:-music-browser}"
NAMESPACE="${NAMESPACE:-music-browser}"
HOSTNAME_FQDN="${HOSTNAME_FQDN:-music.home.arpa}"
HTTP_PORT="${HTTP_PORT:-80}" HTTPS_PORT="${HTTPS_PORT:-443}" DNS_PORT="${DNS_PORT:-53}"
API_PORT="${API_PORT:-6550}"
K3D_VERSION="${K3D_VERSION:-v5.9.0}"
# k3s 1.34: Kubernetes 1.35+ refuses to start on cgroup v1 hosts by default.
K3S_IMAGE="${K3S_IMAGE:-rancher/k3s:v1.34.13-k3s1}"
ISTIO_VERSION="${ISTIO_VERSION:-1.30.8}"
IMAGE_REGISTRY="${IMAGE_REGISTRY:-ghcr.io/asteroiddavis}"
IMAGE_TAG="${IMAGE_TAG:-mini-local}"
KUBECONFIG="${KUBECONFIG_MINI:-$REPO/mini-kubeconfig}"
export KUBECONFIG

declare -A K3D_SHA256=(
    [amd64]=06d8f25bc3a971c4eb29e0ff08429b180402db0f4dec838c9eac427e296800a0
    [arm64]=03cde5cf23e6e8e67de5a039ecf26e5b85aca82fba3e5d13dadf904cd218a250
)

log() { printf '==> %s\n' "$*" >&2; }
die() { printf 'mini: %s\n' "$*" >&2; exit 1; }

# Run a shell snippet on the host with MINI_ROOT/bin and docker on PATH.
remote() {
    [ -n "$HOST" ] || die "set HOST=admin@<host-ip> (or HOST=local)"
    local prelude="export PATH=$MINI_ROOT/bin:${DOCKER_DIR:+$DOCKER_DIR:}\$PATH; set -e;"
    if [ "$HOST" = local ]; then
        bash -c "$prelude $*"
    else
        # shellcheck disable=SC2029  # expand locally on purpose
        ssh "$HOST" "$prelude $*"
    fi
}

host_arch() {
    case "$(remote uname -m)" in
    x86_64 | amd64) echo amd64 ;;
    aarch64 | arm64) echo arm64 ;;
    *) die "unsupported host arch $(remote uname -m)" ;;
    esac
}

# busybox on the host with the data dir at /d (native arch, no cached foreign image).
busybox() {
    remote "docker run --rm --platform linux/$(host_arch) -v $MINI_ROOT/data:/d busybox:1.36 $*"
}

lan_ip() {
    [ -n "${LAN_IP:-}" ] && { echo "$LAN_IP"; return; }
    remote "ip route get 1.1.1.1 2>/dev/null | awk '{for(i=1;i<NF;i++) if(\$i==\"src\"){print \$(i+1); exit}}'"
}

cmd_tools() {
    local arch
    arch="$(host_arch)"
    log "installing k3d $K3D_VERSION ($arch) into $MINI_ROOT/bin"
    remote "mkdir -p $MINI_ROOT/bin && cd $MINI_ROOT/bin &&
        curl -fsSLo k3d.tmp https://github.com/k3d-io/k3d/releases/download/$K3D_VERSION/k3d-linux-$arch &&
        echo '${K3D_SHA256[$arch]}  k3d.tmp' | sha256sum -c - >/dev/null &&
        chmod +x k3d.tmp && mv k3d.tmp k3d && k3d version | head -1 && docker version --format 'docker {{.Server.Version}}'"
}

cmd_tunnel() {
    [ "$HOST" = local ] && return 0
    pkill -f "ssh -fNL $API_PORT:127.0.0.1:$API_PORT" 2>/dev/null || true
    ssh -fNL "$API_PORT:127.0.0.1:$API_PORT" -o ExitOnForwardFailure=yes "$HOST"
}

cmd_up() {
    local ip
    ip="$(lan_ip)"
    [ -n "$ip" ] || die "could not find the host LAN IP; set LAN_IP"
    log "cluster $CLUSTER on $ip (http $HTTP_PORT, https $HTTPS_PORT, dns $DNS_PORT)"
    # Data dirs owned by the app/PocketBase uids so the pods stay unprivileged.
    remote "mkdir -p $MINI_ROOT/data/app $MINI_ROOT/data/pb $MINI_ROOT/backups"
    busybox "sh -c 'chown 10001:10001 /d/app && chown 10002:10002 /d/pb'"
    if ! remote "k3d cluster list $CLUSTER" >/dev/null 2>&1; then
        # k3d node containers need --privileged (k3s runs its own containerd);
        # the app pods inside do not.
        remote "k3d cluster create $CLUSTER --image $K3S_IMAGE --no-lb --wait \
            --api-port 127.0.0.1:$API_PORT \
            -p $ip:$HTTP_PORT:80@server:0:direct -p $ip:$HTTPS_PORT:443@server:0:direct \
            -p $ip:$DNS_PORT:53/udp@server:0:direct -p $ip:$DNS_PORT:53/tcp@server:0:direct \
            -v $MINI_ROOT/data:/var/lib/music-browser@server:0 \
            --k3s-arg --disable=traefik@server:0 \
            --k3s-arg --disable=metrics-server@server:0 \
            --k3s-arg '--kubelet-arg=eviction-hard=memory.available<100Mi@server:0' \
            --kubeconfig-update-default=false --kubeconfig-switch-context=false"
    fi
    remote "k3d kubeconfig get $CLUSTER" >"$KUBECONFIG"
    chmod 600 "$KUBECONFIG"
    cmd_tunnel
    kubectl get nodes -o wide
}

cmd_images() {
    local arch
    arch="$(host_arch)"
    log "building linux/$arch images"
    make -C "$REPO" images PLATFORMS="linux/$arch" IMAGE_PREFIX="$IMAGE_REGISTRY/" IMAGE_TAG="$IMAGE_TAG" \
        MUSIC_OPERATIONS_REF="${MUSIC_OPERATIONS_REF:-main}"
    local imgs="$IMAGE_REGISTRY/music-browser:$IMAGE_TAG $IMAGE_REGISTRY/music-browser-pocketbase:$IMAGE_TAG"
    # shellcheck disable=SC2086
    if [ "$HOST" = local ]; then
        k3d image import -c "$CLUSTER" $imgs
    else
        # shellcheck disable=SC2029
        docker save $imgs | gzip -1 | ssh "$HOST" "gunzip | ${DOCKER_DIR:+$DOCKER_DIR/}docker load"
        remote "k3d image import -c $CLUSTER $imgs"
    fi
}

cmd_istio() {
    helm repo add istio https://istio-release.storage.googleapis.com/charts >/dev/null 2>&1 || true
    helm repo update istio >/dev/null
    local h=(helm upgrade --install --wait --version "$ISTIO_VERSION")
    "${h[@]}" istio-base istio/base -n istio-system --create-namespace
    "${h[@]}" istiod istio/istiod -n istio-system -f "$REPO/deploy/helm/istio/values-istiod-mini.yaml"
    "${h[@]}" istio-ingress istio/gateway -n istio-ingress --create-namespace \
        -f "$REPO/deploy/helm/istio/values-gateway-mini.yaml"
    kubectl label namespace "$NAMESPACE" istio-injection=enabled --overwrite 2>/dev/null ||
        { kubectl create namespace "$NAMESPACE" && kubectl label namespace "$NAMESPACE" istio-injection=enabled; }
}

cmd_deploy() {
    local arch ip chart="$REPO/deploy/helm/music-browser" extra=()
    arch="$(host_arch)"
    ip="$(lan_ip)"
    kubectl -n "$NAMESPACE" get configmap lan-ca >/dev/null 2>&1 && extra+=(--set trust.configMap=lan-ca)
    kubectl -n istio-ingress get secret music-browser-tls >/dev/null 2>&1 ||
        log "warning: no istio-ingress/music-browser-tls yet; run deploy/tls/lan-ca.sh publish $HOSTNAME_FQDN"
    helm upgrade --install music-browser "$chart" -n "$NAMESPACE" --create-namespace --wait --timeout 10m \
        -f "$chart/values-mini.yaml" -f "$chart/values-mini-$arch.yaml" \
        --set hostname="$HOSTNAME_FQDN" \
        --set image.registry="$IMAGE_REGISTRY" --set image.tag="$IMAGE_TAG" \
        --set musicBrowser.persistence.hostPath=/var/lib/music-browser/app \
        --set pocketbase.persistence.hostPath=/var/lib/music-browser/pb \
        --set-string "lanDns.records.${HOSTNAME_FQDN//./\\.}=$ip" \
        "${extra[@]}" "$@"
    cmd_status
}

scale() {
    kubectl -n "$NAMESPACE" scale deploy/music-browser deploy/pocketbase --replicas="$1"
    if [ "$1" = 0 ]; then
        kubectl -n "$NAMESPACE" wait --for=delete pod -l 'app in (music-browser,pocketbase)' --timeout=120s
    else
        kubectl -n "$NAMESPACE" rollout status deploy/music-browser deploy/pocketbase --timeout=300s
    fi
}

cmd_backup() {
    local name
    name="backup-$(date +%Y%m%d-%H%M%S).tgz"
    log "stopping app + PocketBase for a consistent SQLite copy"
    scale 0
    trap 'scale 1' EXIT
    remote "cd $MINI_ROOT/data && tar czf $MINI_ROOT/backups/$name app pb && ls -l $MINI_ROOT/backups/$name"
    trap - EXIT
    scale 1
    log "backup: $MINI_ROOT/backups/$name on the host; copy it off the host too"
}

cmd_restore() {
    local file="${1:?usage: restore <backup.tgz on the host>}"
    remote "tar tzf $file >/dev/null" || die "$file is not a readable backup on the host"
    scale 0
    trap 'scale 1' EXIT
    busybox "rm -rf /d/app.old /d/pb.old"
    remote "cd $MINI_ROOT/data && mv app app.old && mv pb pb.old &&
        tar xzf $file"
    busybox "sh -c 'chown -R 10001:10001 /d/app && chown -R 10002:10002 /d/pb'"
    trap - EXIT
    scale 1
    log "restored $file (previous data in $MINI_ROOT/data/{app,pb}.old)"
}

cmd_upgrade() {
    IMAGE_TAG="${1:?usage: upgrade <image-tag> [helm args]}"
    shift
    cmd_backup
    cmd_deploy "$@"
}

cmd_rollback() {
    helm rollback music-browser -n "$NAMESPACE" --wait
    log "if the new version migrated the database, also: restore <last backup>"
}

cmd_status() {
    kubectl get pods -A -o wide
    remote "free -m; docker stats --no-stream --format '{{.Name}} {{.MemUsage}}' | grep -E 'k3d-$CLUSTER' || true"
}

cmd_disk_bench() {
    log "200 x 4 KiB write+fsync in $MINI_ROOT/data (SQLite commit pattern)"
    busybox "sh -c '
        s=\$(tr -d . </proc/uptime | cut -d\" \" -f1); i=0
        while [ \$i -lt 200 ]; do
            dd if=/dev/zero of=/d/.bench bs=4k count=1 seek=\$i conv=fsync,notrunc 2>/dev/null; i=\$((i + 1))
        done
        e=\$(tr -d . </proc/uptime | cut -d\" \" -f1); rm -f /d/.bench
        # /proc/uptime has 10 ms ticks: (ticks * 10000 us) / 200 writes.
        echo \"avg write+fsync: \$(( (e - s) * 50 )) us\"'"
    log "under ~10 ms per write is fine for this app; well above that, consider an SSD cache"
}

cmd_down() {
    remote "k3d cluster delete $CLUSTER"
    log "data kept in $MINI_ROOT/data and $MINI_ROOT/backups"
}

cmd="${1:-}"
shift || true
case "$cmd" in
tools | up | images | istio | deploy | status | backup | restore | upgrade | rollback | tunnel | down) "cmd_$cmd" "$@" ;;
disk-bench) cmd_disk_bench ;;
*) sed -n '2,22p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
