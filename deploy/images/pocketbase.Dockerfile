# syntax=docker/dockerfile:1
# PocketBase with the release zip checksum-verified at build time
# (the k3d manifest downloads it at pod start instead).
#   docker buildx build --platform linux/amd64,linux/arm64 \
#     -f deploy/images/pocketbase.Dockerfile .
ARG ALPINE_VERSION=3.20

FROM alpine:${ALPINE_VERSION} AS fetch
ARG POCKETBASE_VERSION=0.38.0
ARG TARGETARCH
RUN set -eu; \
    zip="pocketbase_${POCKETBASE_VERSION}_linux_${TARGETARCH}.zip"; \
    base="https://github.com/pocketbase/pocketbase/releases/download/v${POCKETBASE_VERSION}"; \
    cd /tmp; \
    wget -q "$base/$zip" "$base/checksums.txt"; \
    grep " $zip\$" checksums.txt | sha256sum -c -; \
    unzip -q "$zip" pocketbase -d /opt/pb

FROM alpine:${ALPINE_VERSION}
RUN apk add --no-cache ca-certificates tini \
 && addgroup -g 10002 pb && adduser -D -u 10002 -G pb -h /pb_data pb
COPY --from=fetch /opt/pb/pocketbase /usr/local/bin/pocketbase
# CI-only seed data stays out of the deployable image.
COPY --chown=pb:pb pocketbase/pb_migrations/ /pb_migrations/
RUN rm -f /pb_migrations/*_ci_test_seed.js
USER 10002:10002
VOLUME ["/pb_data"]
EXPOSE 8090
ENTRYPOINT ["/sbin/tini", "--"]
CMD ["pocketbase", "serve", "--http=0.0.0.0:8090", "--dir=/pb_data", "--migrationsDir=/pb_migrations"]
