# syntax=docker/dockerfile:1
# Multi-arch image: music-browser + bulk-import, music_operations and repomix.
#   docker buildx build --platform linux/amd64,linux/arm64 \
#     -f deploy/images/music-browser.Dockerfile .
# Rust is cross-compiled on the build host (no QEMU for the slow part).
# Export just the binaries with --target binaries --output type=local,dest=dist.

ARG RUST_VERSION=1.97
ARG NODE_VERSION=22

FROM --platform=$BUILDPLATFORM rust:${RUST_VERSION}-slim-bookworm AS build
ARG TARGETARCH
ARG BUILDARCH
RUN set -eu; \
    case "$TARGETARCH" in \
      amd64) gnu=x86_64-linux-gnu; cross=gcc-x86-64-linux-gnu ;; \
      arm64) gnu=aarch64-linux-gnu; cross=gcc-aarch64-linux-gnu ;; \
      *) echo "unsupported TARGETARCH $TARGETARCH" >&2; exit 1 ;; \
    esac; \
    [ "$TARGETARCH" != "$BUILDARCH" ] || cross=gcc; \
    dpkg --add-architecture "$TARGETARCH"; \
    apt-get update -qq; \
    apt-get install -y -qq --no-install-recommends pkg-config cmake make "$cross" \
      "libc6-dev:$TARGETARCH" "libssl-dev:$TARGETARCH" >/dev/null; \
    rm -rf /var/lib/apt/lists/*; \
    rustup target add "${gnu%%-*}-unknown-linux-gnu"; \
    echo "${gnu%%-*}-unknown-linux-gnu" > /rust-target; \
    echo "$gnu" > /gnu-triple
WORKDIR /src
COPY music_browser/ ./
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target,id=music-browser-target-${TARGETARCH} \
    set -eu; \
    target="$(cat /rust-target)"; gnu="$(cat /gnu-triple)"; \
    tvar="$(echo "$target" | tr 'a-z-' 'A-Z_')"; \
    export "CARGO_TARGET_${tvar}_LINKER=${gnu}-gcc" "CC_$(echo "$target" | tr - _)=${gnu}-gcc"; \
    export PKG_CONFIG_ALLOW_CROSS=1 PKG_CONFIG_PATH="/usr/lib/${gnu}/pkgconfig"; \
    export OPENSSL_LIB_DIR="/usr/lib/${gnu}" OPENSSL_INCLUDE_DIR=/usr/include; \
    cargo build --locked --release --target "$target" --bin music-browser --bin bulk-import; \
    mkdir -p /out; \
    cp "target/$target/release/music-browser" "target/$target/release/bulk-import" /out/

FROM scratch AS binaries
COPY --from=build /out/ /

# Node base so repomix (node >= 22) runs; Debian's python3 (3.11) runs music_operations.
FROM node:${NODE_VERSION}-bookworm-slim AS tools
ARG MUSIC_OPERATIONS_REF=main
ARG REPOMIX_VERSION=1.19.0
RUN apt-get update -qq \
 && apt-get install -y -qq --no-install-recommends git ca-certificates python3 python3-venv >/dev/null \
 && python3 -m venv /opt/music-operations \
 && /opt/music-operations/bin/pip install -q --no-cache-dir \
      "music-operations @ git+https://github.com/asteroidDavis/music_operations@${MUSIC_OPERATIONS_REF}" \
 && npm install -g --omit=dev --no-fund --no-audit "repomix@${REPOMIX_VERSION}" \
 && npm cache clean --force

FROM node:${NODE_VERSION}-bookworm-slim
RUN apt-get update -qq \
 && apt-get install -y -qq --no-install-recommends ca-certificates libssl3 python3 tini >/dev/null \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd -g 10001 app && useradd -u 10001 -g app -d /data -s /usr/sbin/nologin app \
 && mkdir -p /data /music && chown app:app /data
COPY --from=tools /opt/music-operations /opt/music-operations
COPY --from=tools /usr/local/lib/node_modules/repomix /usr/local/lib/node_modules/repomix
RUN ln -s /usr/local/lib/node_modules/repomix/bin/repomix.cjs /usr/local/bin/repomix \
 && ln -s /opt/music-operations/bin/music-operations /usr/local/bin/music-operations
COPY --from=build /out/music-browser /out/bulk-import /usr/local/bin/
ENV BIND_ADDR=0.0.0.0:3000 \
    DATABASE_URL=sqlite:/data/music_browser.db \
    MUSIC_OPERATIONS_BIN=/usr/local/bin/music-operations \
    RUST_LOG=info
USER 10001:10001
WORKDIR /data
VOLUME ["/data"]
EXPOSE 3000
ENTRYPOINT ["/usr/bin/tini", "--"]
CMD ["music-browser"]
