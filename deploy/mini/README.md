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

# mini deploy runbook

`mini.sh` runs on your laptop and drives the host over SSH. The host only
needs Docker; k3d is installed into `MINI_ROOT/bin` (default
`~/music-browser`, put it on a data volume so firmware updates keep it).
kubectl, helm and docker run locally. `HOST=local` runs everything on this
machine (used for the local dry run).

Only infrastructure is privileged: k3d runs the k3s node as a privileged
container because k3s runs its own containerd. The music browser,
PocketBase and LAN CoreDNS pods run as non-root with a read-only root
filesystem, `RuntimeDefault` seccomp and all capabilities dropped (CoreDNS
keeps `NET_BIND_SERVICE` in its bounding set only because `/coredns`
carries that file capability; it listens on 5353).

## 0. Ports 53, 80 and 443

The cluster binds only `LAN_IP` (auto-detected; override with `LAN_IP=`).
If another service holds the port on all addresses (`0.0.0.0`), `up` fails.
`make mini-preflight` lists the holder of each port. Options:

| Port | Typical holder | Options |
| --- | --- | --- |
| 80, 443 | the host's admin UI / web server | move the admin UI to other ports and turn the built-in web server off, or deploy on `HTTP_PORT=8080 HTTPS_PORT=8443` (URL becomes `https://music.home.arpa:8443`) |
| 53 | a local dnsmasq / resolver | if it binds only `127.0.x.x` there is no conflict; otherwise free it, or add the `music.home.arpa` record on the router and `deploy --set lanDns.enabled=false` |

Clients find the resolver on port 53 only, so for LAN DNS through CoreDNS
port 53 on `LAN_IP` must be free. Then set the router's DHCP DNS server to
`LAN_IP` with no secondary: a secondary resolver does not know
`music.home.arpa`, so devices that pick it fail to resolve it. While the
host is down, LAN name lookups fail.

## 1. First deploy

```sh
export HOST=admin@10.0.0.124            # or HOST=local
deploy/mini/mini.sh tools               # pinned, checksummed k3d on the host
deploy/mini/mini.sh up                  # k3d cluster (k3s 1.34: cgroup v1 ok)
deploy/mini/mini.sh images              # build for the host arch, load there
deploy/mini/mini.sh istio               # Istio 1.30 with mini values
deploy/tls/lan-ca.sh init 10.0.0.0/24   # once, on the laptop (see deploy/tls)
deploy/tls/lan-ca.sh issue music.home.arpa 10.0.0.124
KUBECONFIG=$PWD/mini-kubeconfig deploy/tls/lan-ca.sh publish music.home.arpa
deploy/mini/mini.sh deploy              # helm: values-mini + values-mini-<arch>
```

Then open `http://music.home.arpa/trust` on each device, install the CA, and
use `https://music.home.arpa`. Sign-up and login go through PocketBase in
the same release.

`images` builds locally (`docker buildx` with QEMU for a foreign arch) and
streams them to the host, so the host never needs registry credentials. To
use published images instead, set `IMAGE_TAG=<release tag>` and skip
`images`. `MUSIC_OPERATIONS_REF` picks the music_operations git ref baked in
(default `main`).

Persistent data lives on the host in `MINI_ROOT/data/app` (SQLite, uploads)
and `MINI_ROOT/data/pb` (PocketBase), mounted into the node and the pods as
fixed `hostPath`s. Deleting pods, the cluster (`down`) or restarting the host
keeps it. App and PocketBase migrations run on startup.

## 2. Day to day

| Command | What it does |
| --- | --- |
| `status` | pods, host `free -m`, node container memory |
| `backup` | scale app + PocketBase to 0, tar both data dirs to `MINI_ROOT/backups/backup-<time>.tgz`, scale back up (about 30 s downtime) |
| `restore <file>` | scale to 0, keep the current data as `app.old`/`pb.old`, unpack the backup, fix ownership, scale up |
| `upgrade <tag>` | `backup`, then `deploy` with `IMAGE_TAG=<tag>` |
| `rollback` | `helm rollback` to the previous release; if the new version migrated the database, also `restore` the backup `upgrade` took |
| `disk-bench` | 200 x 4 KiB write+fsync in the data dir |
| `tunnel` | reopen the SSH tunnel to the kube API (`127.0.0.1:6550`) |
| `down` | delete the cluster; data and backups stay |

Copy backups off the host (`scp $HOST:music-browser/backups/*.tgz .`).
Run a restore drill after the first deploy and after upgrades that change
the schema: back up, sign up a throwaway user, restore, and check the
throwaway user is gone while your account still logs in.

## 3. Sizing checks

- **Memory:** `status` after an idle minute. On the local amd64 dry run the
  pods' working sets were about 230 MiB together (istiod 48, three
  istio-proxies about 30 each, music-browser 17, PocketBase 10, two CoreDNS
  13 each) and the whole k3d node about 710 MiB including k3s. A host with
  about 1 GB available fits; keep other apps' memory in mind. Swap on HDD
  makes everything slow, so watch `free -m` rather than relying on it.
- **Disk:** `disk-bench`. Under about 10 ms per write+fsync is fine for this
  app's SQLite and PocketBase traffic; well above that, add an SSD cache or
  put `MINI_ROOT` on an SSD volume.

## 4. Troubleshooting

| Symptom | Fix |
| --- | --- |
| `up`: `port is already allocated` / `address already in use` | see section 0 |
| pods `Pending` with memory pressure | stop other containers or lower values in `values-mini.yaml` |
| `curl: (60)` / browser warning | the device does not trust the LAN CA: `/trust` |
| sheet-music job shows `(unavailable)` | expected on arm64 (no AnthemScore build) and on hosts without AnthemScore |
| kubectl `connection refused` | `mini.sh tunnel` |
