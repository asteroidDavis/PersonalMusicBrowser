# Human docs — music_ara_client

Short, opinionated notes for the person maintaining this crate.

## What problem this solves

You're tracking songs, practice sessions, and production in the `music_browser`
Rust app. The heavy-lifting (AnthemScore, hitpoint detection, repomix) is
dispatched by `music_browser` via the job queue. Before Phase 4 the only way
to kick off a job was to leave Cubase, open the browser UI, and paste a
file path. That's friction you pay every time you produce a clip.

This plugin lives inside Cubase's Lower Zone (and REAPER's FX chain for CI).
Pick a clip → pick an operation → click send. Never leave the DAW.

## Normal workflow

1. Launch `music_browser` once per session (`cargo run` from
   `music_browser/`). It binds `http://localhost:3000`.
2. In Cubase: select an audio event, open the Lower Zone, pick the
   **Send To Music Browser** plugin (installed as VST3 ARA effect).
3. Confirm the "Selected clip:" line shows the correct absolute path.
   If it says `(no ARA selection ...)`, click the clip in the arrange
   window and hit **Refresh selection**.
4. Choose the operation you want.
5. Click **Send to Music Browser**.
6. Tab over to `http://localhost:3000/jobs` to watch progress, or stay in
   the DAW — the job runs in the background.

## When the plugin can't extract a path

Some hosts keep the clip's file path in an opaque audio-source property we
can't read. The status bar will say `refusing to POST: empty file path`.
Workarounds:

- In Cubase: ensure the event references an on-disk file, not a recorded
  but unsaved pool clip. Save the project (or use **Audio → Bounce
  Selection**) so the pool entry has a real file.
- In REAPER: the plugin reads `PCM_Source` paths directly and this is
  almost always reliable.

## Updating the operation list

Wire names are defined once in the Rust hub
(`music_browser/src/jobs.rs::Operation`) and mirrored in
`music_ara_client/src/HubClient.{h,cpp}::operationWireName`. Keep them in
lock-step; the round-trip test
`OperationWire.RoundTripsAllVariants` will fail if they drift *within*
C++, and the hub will 400 if they drift *across* languages.

## REAPER license

Buy the $60 discounted license from reaper.fm once you're past evaluating
this project. That covers CI + local dev on this machine indefinitely.
Nothing else to do — no keys to paste into CI.
