# music_ara_client — Send To Music Browser (Phase 4)

JUCE ARA plugin that lives inside a DAW (Cubase target; REAPER used for CI)
and posts the absolute file path of the selected audio clip to the Rust
`music_browser` hub over HTTP, so the user never leaves the DAW.

This is **Phase 4** of the DAW-integration roadmap (issue #4, PR #25).  The
hub was built in earlier phases; this crate is the last big chunk of work
before the Phase 5 user-tool testing pass.

## Layout

```
music_ara_client/
├── CMakeLists.txt           # two build modes: core tests (fast), full plugin (slow)
├── src/
│   ├── ARAFilePathExtractor.{h,cpp}   # pure C++ — host-convention path rules
│   ├── HubClient.{h,cpp}              # pure C++ — JSON payload + POST orchestration
│   ├── PluginProcessor.{h,cpp}        # JUCE ARA effect processor
│   └── PluginEditor.{h,cpp}           # Cubase Lower-Zone UI (dropdown + button)
├── tests/                             # GoogleTest, host-free, always run in CI
├── scripts/reaper_ci.sh               # REAPER-based ARA smoke test
├── docs/
│   ├── humans/   # how a musician or maintainer uses this
│   └── agents/   # how a future coding agent reasons about this
├── .clang-format
└── .pre-commit-config.yaml
```

## Build — fast (host-free unit tests only)

```bash
cmake -S music_ara_client -B music_ara_client/build \
      -DMUSIC_ARA_BUILD_PLUGIN=OFF -DMUSIC_ARA_BUILD_TESTS=ON
cmake --build music_ara_client/build --target sendtohub_core_tests -j
ctest --test-dir music_ara_client/build --output-on-failure
```

## Build — full (JUCE + ARA SDK fetched from GitHub, ~10 min first time)

```bash
cmake -S music_ara_client -B music_ara_client/build-plugin \
      -DMUSIC_ARA_BUILD_PLUGIN=ON -DMUSIC_ARA_BUILD_TESTS=OFF \
      -DCMAKE_BUILD_TYPE=Release
cmake --build music_ara_client/build-plugin --target SendToHubPlugin_VST3 -j
```

The resulting VST3 lands under
`music_ara_client/build-plugin/SendToHubPlugin_artefacts/Release/VST3/`.

Install it into your DAW:

- **Cubase / Nuendo (macOS)** — copy to `~/Library/Audio/Plug-Ins/VST3/`.
- **Cubase / Nuendo (Windows)** — copy to `%CommonProgramFiles%\VST3\`.
- **REAPER (Linux)** — copy to `~/.vst3/`.

## Runtime usage

1. Start the hub: `cargo run --manifest-path music_browser/Cargo.toml`
   (listens on `http://localhost:3000`).
2. In the DAW, select an audio clip in the timeline.
3. Open the plugin in the DAW's ARA / Lower Zone (Cubase) or FX chain (REAPER).
4. Pick **Generate Sheet Music**, **Add Hitpoints**, or **Repomix** from the
   dropdown.
5. Click **Send to Music Browser**.  The plugin returns immediately; hub
   status updates show at `http://localhost:3000/jobs`.

## Contract with the Rust hub

POST body the plugin sends (validated by the GoogleTest suite):

```json
{
  "target_type": "file",
  "target_id_or_path": "/abs/path/to/clip.wav",
  "operation": "generate_sheet_music"
}
```

Matches `WorkflowRequest` in `@/Users/nate/Library/CloudStorage/OneDrive-Personal/Software/PersonalMusicBrowser/music_browser/src/main.rs:1908-1914`.

## REAPER in CI

`scripts/reaper_ci.sh` downloads REAPER (~15 MB), installs the plugin, and
runs a ReaScript probe under `xvfb-run` that asserts the plugin loads and
(optionally, when extended) that ARA exposes the fixture wav's path.

REAPER licensing for CI: one-time $60 discounted license covers personal
automated use.  No per-run or per-minute fees.  See the top-level PR
discussion for details.

## CI

See `@/Users/nate/Library/CloudStorage/OneDrive-Personal/Software/PersonalMusicBrowser/.github/workflows/ci.yml` for the full pipeline.
The `summary` job renders a rust-vs-c++ timing table into
`$GITHUB_STEP_SUMMARY` at the end of every run.
