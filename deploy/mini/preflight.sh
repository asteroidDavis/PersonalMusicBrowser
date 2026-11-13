#!/bin/sh
# Read-only preflight for running the music browser stack on a small
# (`mini`) host such as a NAS or single-board computer.
#
# Run it on the host (POSIX sh, so BusyBox-based firmware works too):
#   ssh admin@<host> 'sh -s' < deploy/mini/preflight.sh > mini-preflight.txt
#   make mini-preflight HOST=admin@<host>
#
# It only reads system state. With --container-tests it also starts two
# short-lived throwaway containers (docker run --rm) to check privileged
# mode and cgroups inside a container; nothing else is changed.
#
# The last block (between "BEGIN SUMMARY" and "END SUMMARY") is KEY=VALUE
# lines meant to be pasted back for the mini Helm values.

CONTAINER_TESTS=0
for arg in "$@"; do
    case "$arg" in
        --container-tests) CONTAINER_TESTS=1 ;;
        -h | --help)
            sed -n '2,13p' "$0" 2>/dev/null || echo "usage: preflight.sh [--container-tests]"
            exit 0
            ;;
        *)
            echo "unknown argument: $arg" >&2
            exit 2
            ;;
    esac
done

# Vendor app-package binaries are not always on a non-login PATH.
PATH="$PATH:/usr/local/bin:/usr/local/sbin:/sbin:/usr/sbin:/bin:/usr/bin"

# Memory budget for the whole cluster (k3s + Istio + apps) plus host OS headroom.
MIN_MEM_TOTAL_MB=3500
MIN_MEM_AVAILABLE_MB=1500
MIN_FREE_DISK_GB=20

SUMMARY=""
FAILS=0
WARNS=0

section() { printf '\n== %s ==\n' "$*"; }
info() { printf '  %s\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }
summary() { SUMMARY="${SUMMARY}$1=$2
"; }
pass() { printf '  [PASS] %s\n' "$*"; }
warn() {
    printf '  [WARN] %s\n' "$*"
    WARNS=$((WARNS + 1))
}
fail() {
    printf '  [FAIL] %s\n' "$*"
    FAILS=$((FAILS + 1))
}

# Some NAS firmwares ship getcfg as their INI reader; fall back to awk.
cfg() { # section key file
    if have getcfg; then
        getcfg "$1" "$2" -f "$3" 2>/dev/null
    elif [ -r "$3" ]; then
        awk -F' *= *' -v s="[$1]" -v k="$2" \
            '$0==s{f=1;next} /^\[/{f=0} f&&$1==k{print $2;exit}' "$3"
    fi
}

meminfo_mb() { awk -v k="$1:" '$1==k{printf "%d", $2/1024}' /proc/meminfo 2>/dev/null; }

# --------------------------------------------------------------------------
section "System"
VENDOR_CONF=/etc/config/uLinux.conf
MODEL="$(cat /sys/firmware/devicetree/base/model 2>/dev/null | tr -d '\0')"
[ -n "$MODEL" ] || MODEL="$(cat /sys/class/dmi/id/product_name 2>/dev/null)"
[ -n "$MODEL" ] || MODEL="$(cfg System Model "$VENDOR_CONF")"
OS_VERSION="$(awk -F= '$1=="PRETTY_NAME"{gsub(/"/, "", $2); print $2}' /etc/os-release 2>/dev/null)"
if [ -z "$OS_VERSION" ] && [ -r "$VENDOR_CONF" ]; then
    OS_VERSION="$(cfg System Version "$VENDOR_CONF") build $(cfg System 'Build Number' "$VENDOR_CONF")"
fi
ARCH="$(uname -m)"
KERNEL="$(uname -r)"
PAGESIZE="$(getconf PAGESIZE 2>/dev/null || getconf PAGE_SIZE 2>/dev/null)"
# BusyBox often has no getconf; smaps reports the kernel page size in kB.
[ -n "$PAGESIZE" ] || PAGESIZE="$(awk '/^KernelPageSize:/{print $2 * 1024; exit}' /proc/self/smaps 2>/dev/null)"
NPROC="$(grep -c '^processor' /proc/cpuinfo 2>/dev/null)"
CPU_PART="$(awk -F': *' '/^CPU part/{print $2;exit}' /proc/cpuinfo 2>/dev/null)"
info "model:        ${MODEL:-unknown}"
info "os:           ${OS_VERSION:-unknown}"
info "kernel:       $KERNEL"
info "arch:         $ARCH"
info "cpus:         ${NPROC:-?}${CPU_PART:+ (CPU part $CPU_PART; 0xd05 = Cortex-A55)}"
info "page size:    ${PAGESIZE:-?}"
info "date (UTC):   $(date -u '+%Y-%m-%dT%H:%M:%SZ')"
summary MODEL "${MODEL:-unknown}"
summary OS_VERSION "${OS_VERSION:-unknown}"
summary ARCH "$ARCH"
summary KERNEL "$KERNEL"
summary PAGESIZE "${PAGESIZE:-unknown}"
summary NPROC "${NPROC:-unknown}"

case "$ARCH" in
    aarch64 | arm64) pass "64-bit ARM: use linux/arm64 images" ;;
    x86_64) pass "x86_64: use linux/amd64 images" ;;
    *) fail "unsupported arch $ARCH (need aarch64 or x86_64)" ;;
esac
if [ "$PAGESIZE" = "4096" ]; then
    pass "4K pages"
else
    # Envoy (Istio) and jemalloc/tcmalloc builds can crash on 16K/64K pages.
    warn "page size ${PAGESIZE:-unknown}: Envoy/Istio and some arm64 images assume 4K pages"
fi

# --------------------------------------------------------------------------
section "Memory"
MEM_TOTAL="$(meminfo_mb MemTotal)"
MEM_AVAIL="$(meminfo_mb MemAvailable)"
SWAP_TOTAL="$(meminfo_mb SwapTotal)"
info "total:        ${MEM_TOTAL:-?} MB"
info "available:    ${MEM_AVAIL:-?} MB"
info "swap:         ${SWAP_TOTAL:-?} MB"
summary MEM_TOTAL_MB "${MEM_TOTAL:-unknown}"
summary MEM_AVAILABLE_MB "${MEM_AVAIL:-unknown}"
summary SWAP_TOTAL_MB "${SWAP_TOTAL:-unknown}"
if [ -n "$MEM_TOTAL" ] && [ "$MEM_TOTAL" -ge "$MIN_MEM_TOTAL_MB" ]; then
    pass "total memory >= ${MIN_MEM_TOTAL_MB} MB"
else
    fail "total memory ${MEM_TOTAL:-?} MB < ${MIN_MEM_TOTAL_MB} MB"
fi
if [ -n "$MEM_AVAIL" ] && [ "$MEM_AVAIL" -ge "$MIN_MEM_AVAILABLE_MB" ]; then
    pass "available memory >= ${MIN_MEM_AVAILABLE_MB} MB (budget for the cluster is ~1.2 GB)"
else
    warn "available memory ${MEM_AVAIL:-?} MB < ${MIN_MEM_AVAILABLE_MB} MB: stop other apps/containers or use the Compose fallback"
fi

# --------------------------------------------------------------------------
section "Kernel features (k3s, Istio sidecar iptables redirect)"
CGROUP_VERSION=1
[ -f /sys/fs/cgroup/cgroup.controllers ] && CGROUP_VERSION=2
info "cgroup:       v$CGROUP_VERSION"
summary CGROUP_VERSION "$CGROUP_VERSION"
if [ "$CGROUP_VERSION" = "2" ]; then
    CONTROLLERS="$(cat /sys/fs/cgroup/cgroup.controllers 2>/dev/null)"
else
    CONTROLLERS="$(awk 'NR>1 && $4==1{printf "%s ", $1}' /proc/cgroups 2>/dev/null)"
fi
info "controllers:  $CONTROLLERS"
for c in cpu memory pids; do
    case " $CONTROLLERS " in
        *" $c "*) pass "cgroup controller $c" ;;
        *) fail "cgroup controller $c missing (k3s needs cpu, memory, pids)" ;;
    esac
done

KCONFIG=""
if [ -r /proc/config.gz ] && have zcat; then
    KCONFIG="$(zcat /proc/config.gz 2>/dev/null)"
fi
# A feature is present if it's loaded, built in, or a module file exists.
kfeature() { # name config_symbol
    if grep -q "^$1 " /proc/modules 2>/dev/null; then
        echo loaded
    elif [ -n "$KCONFIG" ] && printf '%s\n' "$KCONFIG" | grep -q "^$2=y"; then
        echo builtin
    elif [ -n "$KCONFIG" ] && printf '%s\n' "$KCONFIG" | grep -q "^$2=m"; then
        echo module
    elif find "/lib/modules/$KERNEL" -name "$1.ko*" 2>/dev/null | grep -q .; then
        echo module
    else
        echo unknown
    fi
}
if grep -qw overlay /proc/filesystems 2>/dev/null; then
    pass "overlay filesystem"
else
    warn "overlay filesystem not listed in /proc/filesystems"
fi
for pair in br_netfilter:CONFIG_BRIDGE_NETFILTER nf_conntrack:CONFIG_NF_CONNTRACK \
    xt_REDIRECT:CONFIG_NETFILTER_XT_TARGET_REDIRECT xt_owner:CONFIG_NETFILTER_XT_MATCH_OWNER \
    iptable_nat:CONFIG_IP_NF_NAT vxlan:CONFIG_VXLAN; do
    name="${pair%%:*}"
    state="$(kfeature "$name" "${pair#*:}")"
    summary "KMOD_$name" "$state"
    if [ "$state" = unknown ]; then
        warn "$name: not found (needed by k3s flannel/Istio iptables; may be built in without /proc/config.gz)"
    else
        pass "$name: $state"
    fi
done
if [ -e /proc/sys/net/bridge/bridge-nf-call-iptables ]; then
    pass "net.bridge.bridge-nf-call-iptables = $(cat /proc/sys/net/bridge/bridge-nf-call-iptables)"
fi
info "ip_forward:   $(cat /proc/sys/net/ipv4/ip_forward 2>/dev/null)"

# --------------------------------------------------------------------------
section "Docker"
DOCKER=""
if have docker; then
    DOCKER="$(command -v docker)"
else
    # NAS app packages install docker outside PATH.
    for p in /share/*/.qpkg/*/bin/docker /var/packages/*/target/usr/bin/docker /opt/*/bin/docker; do
        if [ -x "$p" ]; then
            DOCKER="$p"
            break
        fi
    done
fi
summary DOCKER_BIN "${DOCKER:-none}"
DOCKER_OK=0
if [ -z "$DOCKER" ]; then
    fail "docker CLI not found (install or enable the container runtime)"
elif ! "$DOCKER" info >/dev/null 2>&1; then
    fail "docker at $DOCKER cannot reach the daemon (run as an admin user, or start the container runtime)"
else
    DOCKER_OK=1
    DOCKER_VERSION="$("$DOCKER" version --format '{{.Server.Version}}' 2>/dev/null)"
    DOCKER_ROOT="$("$DOCKER" info --format '{{.DockerRootDir}}' 2>/dev/null)"
    DOCKER_CGDRIVER="$("$DOCKER" info --format '{{.CgroupDriver}}' 2>/dev/null)"
    DOCKER_STORAGE="$("$DOCKER" info --format '{{.Driver}}' 2>/dev/null)"
    DOCKER_SECOPTS="$("$DOCKER" info --format '{{.SecurityOptions}}' 2>/dev/null)"
    info "docker:       $DOCKER_VERSION ($DOCKER)"
    info "root dir:     $DOCKER_ROOT"
    info "storage:      $DOCKER_STORAGE"
    info "cgroup drv:   $DOCKER_CGDRIVER"
    info "security:     $DOCKER_SECOPTS"
    info "containers:   $("$DOCKER" ps -q 2>/dev/null | wc -l | tr -d ' ') running"
    summary DOCKER_VERSION "$DOCKER_VERSION"
    summary DOCKER_ROOT "$DOCKER_ROOT"
    summary DOCKER_STORAGE "$DOCKER_STORAGE"
    pass "docker daemon reachable"
fi

# --------------------------------------------------------------------------
section "Kubernetes tooling"
K3S_BUILTIN=none
DOCKER_DIR="$(dirname "${DOCKER:-/nonexistent/docker}")"
for p in "$DOCKER_DIR/k3s" "$DOCKER_DIR/../k3s/k3s" /usr/local/bin/k3s; do
    if [ -x "$p" ]; then
        K3S_BUILTIN="$p"
        break
    fi
done
if [ "$K3S_BUILTIN" = none ] && have k3s; then K3S_BUILTIN="$(command -v k3s)"; fi
k3s_pids=""
for c in /proc/[0-9]*/cmdline; do
    cmd="$(tr '\0' ' ' <"$c" 2>/dev/null)"
    case "${cmd%% *}" in
    k3s | */k3s) case "${cmd#* }" in server*) k3s_pids="$k3s_pids ${c#/proc/}" ;; esac ;;
    esac
done
if [ -n "$k3s_pids" ]; then
    K3S_RUNNING=yes
else
    K3S_RUNNING=no
fi
info "k3s binary:   $K3S_BUILTIN (running: $K3S_RUNNING)"
summary K3S_BUILTIN "$K3S_BUILTIN"
summary K3S_RUNNING "$K3S_RUNNING"
for t in k3d kubectl istioctl helm; do
    if have "$t"; then
        info "$t: $(command -v "$t")"
        summary "HAVE_$t" yes
    else
        info "$t: not installed (installed later by the deploy step)"
        summary "HAVE_$t" no
    fi
done

# --------------------------------------------------------------------------
section "Network and ports"
NAS_IPS="$(ip -4 -o addr show scope global 2>/dev/null | awk '{split($4,a,"/"); printf "%s(%s) ", a[1], $2}')"
[ -n "$NAS_IPS" ] || NAS_IPS="$(ifconfig 2>/dev/null | awk '/inet addr:/{sub("addr:","",$2); if ($2!="127.0.0.1") printf "%s ", $2}')"
DEFAULT_GW="$(ip route 2>/dev/null | awk '/^default/{print $3; exit}')"
RESOLVERS="$(awk '/^nameserver/{printf "%s ", $2}' /etc/resolv.conf 2>/dev/null)"
info "ipv4:         $NAS_IPS"
info "gateway:      ${DEFAULT_GW:-?}"
info "resolvers:    $RESOLVERS"
summary NAS_IPV4 "$NAS_IPS"
summary DEFAULT_GATEWAY "${DEFAULT_GW:-unknown}"
summary RESOLVERS "$RESOLVERS"

# "proto local-address process" per listening socket.
LISTEN=""
if have ss; then
    LISTEN="$(ss -Htulnp 2>/dev/null | awk '{p=$7; sub(/^users:\(\("/, "", p); sub(/".*/, "", p); print $1" "$5" "(p == "" ? "?" : p)}')"
elif have netstat; then
    LISTEN="$(netstat -tulnp 2>/dev/null | awk '/^(tcp|udp)/{p=$NF; if (p !~ /\//) p="?/?"; sub(/^[0-9]+\//, "", p); print $1" "$4" "p}')"
fi
# port hint
check_port() {
    if [ -z "$LISTEN" ]; then
        warn "port $1: cannot list listening sockets (no ss/netstat)"
        summary "PORT_$1" unknown
        return
    fi
    holders="$(printf '%s\n' "$LISTEN" | awk -v p="$1" '{n=split($2,a,":"); if (a[n]==p) printf "%s:%s(%s) ", $1, $2, $3}')"
    if [ -n "$holders" ]; then
        warn "port $1 in use: $holders- $2"
        summary "PORT_$1" in-use
        summary "PORT_$1_HOLDERS" "$holders"
    else
        pass "port $1 free"
        summary "PORT_$1" free
    fi
}
check_port 53 "LAN CoreDNS needs it; if holders bind only loopback/bridge addresses, CoreDNS can bind the LAN IP alone"
check_port 80 "used for the /trust CA download page; can move to another port"
check_port 443 "gateway HTTPS; move the host web UI, bind the gateway to a second IP, or use another port"

for url in https://ghcr.io/v2/ https://registry-1.docker.io/v2/ https://github.com/ https://pkgs.tailscale.com/; do
    code=""
    if have curl; then
        code="$(curl -s -o /dev/null -m 10 -w '%{http_code}' "$url" 2>/dev/null)"
    elif have wget; then
        wget -q -T 10 --spider "$url" >/dev/null 2>&1 && code=ok
    fi
    case "$code" in
        "" | 000) warn "cannot reach $url" ;;
        *) pass "reach $url ($code)" ;;
    esac
done

# --------------------------------------------------------------------------
section "Storage"
# One line per real filesystem; -P keeps df from wrapping long device names.
MOUNTS="$(awk '$3 ~ /^(ext[234]|xfs|btrfs|zfs|f2fs)$/ {print $2" "$3}' /proc/mounts 2>/dev/null | sort -u -k1,1)"
[ -n "$DOCKER_ROOT" ] && MOUNTS="$MOUNTS
$DOCKER_ROOT docker-root"
printf '%s\n' "$MOUNTS" | while read -r mnt fstype; do
    [ -d "$mnt" ] || continue
    free_gb="$(df -Pk "$mnt" 2>/dev/null | awk 'END{printf "%d", $(NF-2)/1048576}')"
    size_gb="$(df -Pk "$mnt" 2>/dev/null | awk 'END{printf "%d", $(NF-4)/1048576}')"
    info "$mnt ($fstype): ${free_gb:-?} GB free of ${size_gb:-?} GB"
    if [ -n "$free_gb" ] && [ "$free_gb" -lt "$MIN_FREE_DISK_GB" ]; then
        printf '  [WARN] %s has less than %s GB free\n' "$mnt" "$MIN_FREE_DISK_GB"
    fi
done
VOLUMES="$(printf '%s\n' "$MOUNTS" | while read -r mnt fstype; do
    [ -d "$mnt" ] && printf '%s:%s:%sG ' "$mnt" "$fstype" "$(df -Pk "$mnt" 2>/dev/null | awk 'END{printf "%d", $(NF-2)/1048576}')"
done)"
summary VOLUMES "$VOLUMES"
DISKS=""
for b in /sys/block/sd* /sys/block/nvme*; do
    [ -e "$b/queue/rotational" ] || continue
    if [ "$(cat "$b/queue/rotational")" = "1" ]; then kind=hdd; else kind=ssd; fi
    DISKS="$DISKS$(basename "$b"):$kind "
done
info "disks:        ${DISKS:-none found}"
summary DISKS "$DISKS"
case "$DISKS" in
    *ssd*) pass "an SSD is present (put pb_data/app data there if it's a separate volume)" ;;
    *hdd*) info "HDD only: fine for SQLite at this size; expect a slow first request after spin-down" ;;
esac

# --------------------------------------------------------------------------
if [ "$CONTAINER_TESTS" = 1 ] && [ "$DOCKER_OK" = 1 ]; then
    section "Container tests (--container-tests)"
    "$DOCKER" pull -q alpine:3.20 >/dev/null 2>&1
    if out="$("$DOCKER" run --rm alpine:3.20 sh -c 'uname -m; cat /sys/fs/cgroup/cgroup.controllers 2>/dev/null || echo v1' 2>&1)"; then
        pass "unprivileged container runs: $(echo "$out" | tr '\n' ' ')"
        summary CONTAINER_RUN ok
    else
        fail "cannot run a container: $out"
        summary CONTAINER_RUN fail
    fi
    if out="$("$DOCKER" run --rm --privileged alpine:3.20 sh -c 'mount -t tmpfs t /mnt && echo mounted' 2>&1)" \
        && echo "$out" | grep -q mounted; then
        pass "privileged containers allowed (k3d needs them)"
        summary PRIVILEGED ok
    else
        fail "privileged container failed (k3d cannot run): $out"
        summary PRIVILEGED fail
    fi
fi

# --------------------------------------------------------------------------
section "Verdict"
case "$ARCH" in aarch64 | arm64 | x86_64) arch_ok=1 ;; *) arch_ok=0 ;; esac
if [ "$arch_ok" = 1 ] && [ "$DOCKER_OK" = 1 ] && [ "$FAILS" = 0 ]; then
    if [ "$CONTAINER_TESTS" = 1 ]; then
        V_K3D=go
    else
        V_K3D=likely
    fi
else
    V_K3D=no-go
fi
if [ "$K3S_BUILTIN" != none ]; then V_K3S=available; else V_K3S=not-found; fi
if [ "$arch_ok" = 1 ] && [ "$DOCKER_OK" = 1 ]; then V_COMPOSE=go; else V_COMPOSE=no-go; fi
info "k3d on docker:            $V_K3D$([ "$V_K3D" = likely ] && echo ' (rerun with --container-tests to confirm privileged mode)')"
info "preinstalled k3s:         $V_K3S"
info "Docker Compose fallback:  $V_COMPOSE"
info "$FAILS fail, $WARNS warn"
summary VERDICT_K3D "$V_K3D"
summary VERDICT_PREINSTALLED_K3S "$V_K3S"
summary VERDICT_COMPOSE "$V_COMPOSE"
summary FAILS "$FAILS"
summary WARNS "$WARNS"

printf '\n-----BEGIN SUMMARY-----\n%s-----END SUMMARY-----\n' "$SUMMARY"
exit 0
