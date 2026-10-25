# QNAP preflight

`preflight.sh` checks whether a QNAP NAS (target: TS-433, ARM Cortex-A55, 4 GB)
can run the music browser stack, and which platform to use:

1. k3d on Container Station's Docker (primary)
2. Container Station's built-in K3s
3. Docker Compose (fallback)

It is POSIX `sh` (QTS ships BusyBox) and read-only: it installs nothing and
changes no QTS, firewall, DNS or storage settings.

## Run

Enable SSH in QTS (Control Panel > Network & File Services > Telnet / SSH), then
from a checkout of this repo:

```sh
make qnap-preflight NAS=admin@<nas-ip>
# also start two throwaway `docker run --rm` containers to check privileged mode:
make qnap-preflight NAS=admin@<nas-ip> PREFLIGHT_ARGS=--container-tests
```

or without make:

```sh
ssh admin@<nas-ip> 'sh -s -- --container-tests' < deploy/qnap/preflight.sh > qnap-preflight.txt
```

Output is saved to `qnap-preflight.txt` (git-ignored; it lists LAN addresses).
The block between `-----BEGIN SUMMARY-----` and `-----END SUMMARY-----` is
`KEY=VALUE` lines used to size the QNAP Helm values.

## What it checks

| Area | Checks |
| --- | --- |
| System | model, QTS version, arch (`aarch64` / `x86_64`), page size (Envoy needs 4K) |
| Memory | total >= 3500 MB, available >= 1500 MB, swap |
| Kernel | cgroup v1/v2 controllers, overlay, `br_netfilter`, `nf_conntrack`, `xt_REDIRECT`, `xt_owner`, `iptable_nat`, `vxlan`, IP forwarding |
| Containers | Container Station version/path, Docker version, root dir, storage and cgroup driver |
| Kubernetes | built-in `k3s` binary or running `k3s server`; `k3d`, `kubectl`, `istioctl`, `helm` |
| Network | addresses, gateway, resolvers, whether ports 53/80/443 are free, reachability of ghcr.io, Docker Hub, GitHub, pkgs.tailscale.com |
| Storage | shared folders and disks |

The verdict prints `go`, `likely`, or `no-go` for k3d and Compose, and
`available` or `not-found` for built-in K3s. Warnings for kernel modules can be
false alarms when a feature is built into the kernel and `/proc/config.gz` is
missing; `--container-tests` gives the definitive answer for k3d.
