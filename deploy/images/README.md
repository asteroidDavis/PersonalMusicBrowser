# Images and release binaries

| Image | Contents | User |
| --- | --- | --- |
| `music-browser` | `music-browser`, `bulk-import`, `music-operations` (venv), `repomix` (Node 22) | 10001, unprivileged |
| `music-browser-pocketbase` | PocketBase (release zip checksum-verified at build), `pb_migrations` without the CI seed users | 10002, unprivileged |

Both are built for `linux/amd64` and `linux/arm64`. Rust is cross-compiled on
the build host (`FROM --platform=$BUILDPLATFORM`), so arm64 builds don't run
`cargo` under QEMU.

```sh
make images                                   # both arches, --load
make images PLATFORMS=linux/arm64 IMAGE_TAG=dev
make images IMAGE_PREFIX=ghcr.io/asteroiddavis/ IMAGE_TAG=v0.2.0 PUSH=1
make release-binaries IMAGE_TAG=v0.2.0         # dist/*.tar.gz + SHA256SUMS
```

`MUSIC_OPERATIONS_REF` picks the music_operations git ref installed in the
image (default `main`).

On arm64, `music-operations --list-operations` reports `anthemscore` as
unavailable (AnthemScore has no Linux arm64 build); `repomix` works.
`generate_sheet_music` jobs need an x86_64/Windows/macOS host.

Defaults baked into `music-browser`: `BIND_ADDR=0.0.0.0:3000`,
`DATABASE_URL=sqlite:/data/music_browser.db`. Mount `/data` (and `/pb_data`
for PocketBase) on persistent storage. TLS is terminated by the gateway, so
`HTTPS_ENABLED` stays `false` in the container.
