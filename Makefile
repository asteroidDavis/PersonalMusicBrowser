.PHONY: backup ara-test ara-plugin ara-clean rust-clean rust-run pocketbase-run pocketbase-run-insecure

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

pocketbase-cert:
	@openssl req -x509 -newkey rsa:2048 -keyout pocketbase/localhost.key \
		-out pocketbase/localhost.crt -days 365 -nodes \
		-subj "/CN=127.0.0.1" -addext "subjectAltName=IP:127.0.0.1"
	@echo ""
	@echo "Generated: pocketbase/localhost.crt and pocketbase/localhost.key"
	@echo ""
	@echo "Add these to your music_browser/.env file:"
	@echo "  POCKETBASE_CA_CERT=pocketbase/localhost.crt"
	@echo "  POCKETBASE_URL=https://127.0.0.1:8090"
	@echo ""
	@echo "Then run: make pocketbase-run"

pocketbase-run-insecure:
	@if [ -x pocketbase/pocketbase ]; then \
		POCKETBASE_BIN=pocketbase/pocketbase; \
	else \
		POCKETBASE_BIN=pocketbase; \
	fi; \
	echo "Starting PocketBase on HTTP (insecure, for local development only)."; \
	$$POCKETBASE_BIN serve --dir pocketbase/pb_data --http 127.0.0.1:8090

pocketbase-run:
	@if [ ! -f pocketbase/localhost.crt ] || [ ! -f pocketbase/localhost.key ]; then \
		echo "Error: pocketbase/localhost.crt or pocketbase/localhost.key not found."; \
		echo "Run: make pocketbase-cert"; \
		exit 1; \
	fi
	@if [ -x pocketbase/pocketbase ]; then \
		POCKETBASE_BIN=pocketbase/pocketbase; \
	else \
		POCKETBASE_BIN=pocketbase; \
	fi; \
	echo "Starting PocketBase on HTTPS (beta, untested)."; \
	$$POCKETBASE_BIN serve --dir pocketbase/pb_data --cert pocketbase/localhost.crt --key pocketbase/localhost.key 127.0.0.1:8090
