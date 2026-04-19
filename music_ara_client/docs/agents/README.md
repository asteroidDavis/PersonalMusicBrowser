# Agent docs — music_ara_client

Orientation for future coding agents picking up this crate. Read this
**before** editing anything under `music_ara_client/`.

## Architectural invariants — do not break these

1. **`sendtohub_core` stays JUCE-free and ARA-free.**
   Its two translation units (`HubClient.cpp`, `ARAFilePathExtractor.cpp`)
   depend only on the C++17 standard library. This is what lets
   `cpp-core-tests` run in ~15s with no DAW installed. Anything that needs
   JUCE or the ARA SDK belongs in `PluginProcessor.{h,cpp}` or
   `PluginEditor.{h,cpp}`.

2. **The wire schema is owned by the Rust hub.**
   Source of truth: `WorkflowRequest` in
   `@/Users/nate/Library/CloudStorage/OneDrive-Personal/Software/PersonalMusicBrowser/music_browser/src/main.rs:1908-1914`
   and `Operation::parse` in
   `@/Users/nate/Library/CloudStorage/OneDrive-Personal/Software/PersonalMusicBrowser/music_browser/src/jobs.rs:39-46`.
   When you change either side, update both, *and* update the tests in
   `tests/test_hub_client.cpp`.

3. **The editor thread must never block.**
   The plugin UI runs on the DAW's message thread. HTTP is always off-loaded
   via `juce::Thread::launch` (see `PluginEditor::onSendClicked`). Never
   call `createInputStream` on the message thread.

4. **ARA path extraction is host-specific and rule-driven.**
   Each host convention is a parametrized test case in
   `test_ara_file_path_extractor.cpp`. If you add a host, add a case there
   *first* (red), then extend `extractAbsolutePath` until it's green.

## Build modes

| Flag                         | What it builds                    | Use for                     |
|------------------------------|-----------------------------------|-----------------------------|
| `MUSIC_ARA_BUILD_TESTS=ON`   | GoogleTest unit suite             | every commit, pre-commit, CI default |
| `MUSIC_ARA_BUILD_PLUGIN=ON`  | JUCE ARA VST3 (fetches JUCE+ARA)  | label-gated CI, local dev, release builds |

Never default `MUSIC_ARA_BUILD_PLUGIN=ON` — the first configure pulls
hundreds of MB of JUCE + ARA_SDK sources.

## CI timing discipline

Every CI step that does real work writes its wall-clock duration to a
`rust-<name>.sec` or `cpp-<name>.sec` file and uploads it as a
`timing-*` artifact. The `summary` job downloads all of them, classifies
by filename prefix, sums per side, and renders the table into
`$GITHUB_STEP_SUMMARY`.

**When you add a new CI job, emit a `*.sec` file and upload a
`timing-*` artifact.** Otherwise it won't show up in the rust-vs-c++
rollup and future regressions will go unnoticed.

## Things explicitly out of scope for Phase 4

- Hitpoint markers round-tripping back into the DAW timeline (Phase 5).
- Any non-file `target_type` (songs, live sets) — the DAW only ever talks
  in absolute file paths; the hub owns song/live-set resolution.
- Authentication — the hub is assumed to listen on `localhost` only.
- AAX, AU, standalone builds. VST3 covers Cubase, REAPER, Studio One,
  and FL Studio; add others only when a user needs them.

## Known gotchas

- **JUCE 8 + Linux + ARA**: you need `libwebkit2gtk-4.1-dev`
  (already installed by the `cpp-plugin-build` CI job). Locally,
  `sudo apt-get install libwebkit2gtk-4.1-dev` before configuring.
- **REAPER ReaScript headless mode**: REAPER *does* run `.lua` scripts
  supplied on the CLI, but it still opens a window on X11. That's why the
  CI job runs it under `xvfb-run -a`. There is no documented truly-headless
  mode. If you need deterministic output, always write JSON from the
  script (see `ARA_PROBE_OUT`) rather than screen-scraping the log.
- **ARA audio source persistent ID is host-defined.** Do not assume any
  particular format globally; add a test case per host.

## Pre-commit

`music_ara_client/.pre-commit-config.yaml` registers two hooks:

- `clang-format` (in-place with `--style=file`).
- `cpp-core-tests` — configures + builds + runs the GoogleTest suite.
  Runs on any change under `music_ara_client/`.

The root Rust hooks live in `music_browser/.pre-commit-config.yaml`; the
two configs are independent by design (separate build systems).
