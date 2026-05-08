.PHONY: backup ara-test ara-plugin ara-clean rust-clean rust-run pocketbase-run

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

pocketbase-run:
	@if [ -x pocketbase/pocketbase ]; then \
		POCKETBASE_BIN=pocketbase/pocketbase; \
	else \
		POCKETBASE_BIN=pocketbase; \
	fi; \
	CERT_PATH=$${POCKETBASE_CERT_PATH:-pocketbase/localhost+2.pem}; \
	KEY_PATH=$${POCKETBASE_KEY_PATH:-pocketbase/localhost+2-key.pem}; \
	if [ -f "$$CERT_PATH" ] && [ -f "$$KEY_PATH" ]; then \
		$$POCKETBASE_BIN serve --dir pocketbase/pb_data --https 127.0.0.1:8090 "$$CERT_PATH" "$$KEY_PATH"; \
	else \
		echo "TLS certificate files not found; starting PocketBase over HTTP for bootstrap."; \
		echo "Set POCKETBASE_CERT_PATH and POCKETBASE_KEY_PATH or create pocketbase/localhost+2.pem and pocketbase/localhost+2-key.pem."; \
		$$POCKETBASE_BIN serve --dir pocketbase/pb_data --http 127.0.0.1:8090; \
	fi
