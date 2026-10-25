.PHONY: backup ara-test ara-plugin ara-clean rust-clean rust-run rust-run-secure pocketbase-cert pocketbase-run pocketbase-stop pocketbase-run-insecure qnap-preflight

backup:
	@BRANCH=$$(git rev-parse --abbrev-ref HEAD | sed 's/\//-/g') && \
	COMMIT=$$(git rev-parse --short HEAD) && \
	cp music_browser/music_browser.db "music_browser/music_browser.db.bak.$${BRANCH}.$${COMMIT}" && \
	echo "Backed up database to music_browser/music_browser.db.bak.$${BRANCH}.$${COMMIT}"

# Host-free C++ unit tests for the JUCE ARA plugin core (~15s, no JUCE deps).
ara-test:
	cmake -S music_ara_client -B music_ara_client/build \
	      -DMUSIC_ARA_BUILD_PLUGIN=OFF -DMUSIC_ARA_BUILD_TESTS=ON
	cmake --build music_ara_client/build --target sendtohub_core_tests -j
	ctest --test-dir music_ara_client/build --output-on-failure

# Full JUCE ARA VST3 build (fetches JUCE + ARA SDK; ~10 min first run).
ara-plugin:
	cmake -S music_ara_client -B music_ara_client/build-plugin \
	      -DMUSIC_ARA_BUILD_PLUGIN=ON -DMUSIC_ARA_BUILD_TESTS=OFF \
	      -DCMAKE_BUILD_TYPE=Release
	cmake --build music_ara_client/build-plugin --target SendToHubPlugin_VST3 -j

ara-clean:
	rm -rf music_ara_client/build music_ara_client/build-plugin music_ara_client/build-precommit

# Rust commands
rust-clean:
	@. ~/.cargo/env && cd music_browser && cargo clean
	@rm -rf music_browser/target
	@echo "Cleaned Rust target directories."

rust-run:
	@. ~/.cargo/env && cd music_browser && cargo run --bin music-browser

# Self-signed local CA (pocketbase/ca.crt) + server cert (pocketbase/localhost.crt).
pocketbase-cert:
	@pocketbase/k3d/pocketbase-k3d.sh cert

pocketbase-run-insecure:
	@if [ -x pocketbase/pocketbase ]; then \
		POCKETBASE_BIN=pocketbase/pocketbase; \
	else \
		POCKETBASE_BIN=pocketbase; \
	fi; \
	echo "Starting PocketBase on HTTP (insecure, for local development only)."; \
	$$POCKETBASE_BIN serve --dir pocketbase/pb_data --http 127.0.0.1:8090

# PocketBase has no --cert/--key flags, so HTTPS is provided by an Istio
# ingress gateway on a local k3d cluster using the self-signed CA above.
# Requires docker, k3d, kubectl and istioctl. Serves https://127.0.0.1:8090.
pocketbase-run:
	@pocketbase/k3d/pocketbase-k3d.sh up

pocketbase-stop:
	@pocketbase/k3d/pocketbase-k3d.sh down

# Music browser over HTTPS, talking to `make pocketbase-run` over HTTPS.
rust-run-secure:
	@if [ ! -f music_browser/certs/server.crt ] || [ ! -f music_browser/certs/server.key ]; then \
		cd music_browser && ./generate-cert.sh; \
	fi
	@. ~/.cargo/env && cd music_browser && \
		HTTPS_ENABLED=true \
		CSRF_COOKIE_SECURE=true \
		AUTH_COOKIE_SECURE=true \
		POCKETBASE_URL=https://127.0.0.1:8090 \
		POCKETBASE_CA_CERT=../pocketbase/ca.crt \
		cargo run --bin music-browser

# Read-only checks on a QNAP NAS over SSH, e.g. make qnap-preflight NAS=admin@192.168.1.50
# Add PREFLIGHT_ARGS=--container-tests to also check privileged containers.
qnap-preflight:
	@if [ -z "$(NAS)" ]; then echo "usage: make qnap-preflight NAS=admin@<nas-ip> [PREFLIGHT_ARGS=--container-tests]"; exit 2; fi
	@ssh "$(NAS)" 'sh -s -- $(PREFLIGHT_ARGS)' < deploy/qnap/preflight.sh | tee qnap-preflight.txt
	@echo "Saved qnap-preflight.txt"
