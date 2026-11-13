# mini preflight

`mini` is the deployment profile for small hosts: arm64 or amd64, about 4 GB
of RAM, slow disks, LAN only (a NAS, mini PC or single-board computer).
`preflight.sh` checks whether a host can run the music browser stack and which
platform to use:

1. k3d on the host's Docker (primary)
2. a preinstalled k3s
3. Docker Compose (fallback)

It is POSIX `sh` (BusyBox firmware works) and read-only: it installs nothing
and changes no OS, firewall, DNS or storage settings.

## Run

Enable SSH on the host, then from a checkout of this repo:

```sh
make mini-preflight HOST=admin@<host-ip>
# also start two throwaway `docker run --rm` containers to check privileged mode:
make mini-preflight HOST=admin@<host-ip> PREFLIGHT_ARGS=--container-tests
```

or without make:

```sh
ssh admin@<host-ip> 'sh -s -- --container-tests' < deploy/mini/preflight.sh > mini-preflight.txt
```

Output is saved to `mini-preflight.txt` (git-ignored; it lists LAN addresses).
The block between `-----BEGIN SUMMARY-----` and `-----END SUMMARY-----` is
`KEY=VALUE` lines used to size the mini Helm values.

## What it checks

| Area | Checks |
| --- | --- |
| System | model, OS version, arch (`aarch64` / `x86_64`), page size (Envoy needs 4K) |
| Memory | total >= 3500 MB, available >= 1500 MB, swap |
| Kernel | cgroup v1/v2 controllers, overlay, `br_netfilter`, `nf_conntrack`, `xt_REDIRECT`, `xt_owner`, `iptable_nat`, `vxlan`, IP forwarding |
| Containers | Docker path (including vendor app-package dirs), version, root dir, storage and cgroup driver |
| Kubernetes | preinstalled `k3s` binary or running `k3s server`; `k3d`, `kubectl`, `istioctl`, `helm` |
| Network | addresses, gateway, resolvers, which address/process holds ports 53/80/443, reachability of ghcr.io, Docker Hub, GitHub, pkgs.tailscale.com |
| Storage | free space per real filesystem and the Docker root, disk type (HDD/SSD) |

`--container-tests` is the only check that needs privileged containers: k3d
runs each k3s node as a privileged Docker container. The application pods
inside the cluster run unprivileged (see the Helm chart).

The verdict prints `go`, `likely`, or `no-go` for k3d and Compose, and
`available` or `not-found` for a preinstalled k3s. Kernel-module warnings can be
false alarms when a feature is built into the kernel and `/proc/config.gz` is
missing; `--container-tests` gives the definitive answer for k3d.
