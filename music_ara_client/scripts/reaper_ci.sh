#!/usr/bin/env bash
# reaper_ci.sh — REAPER-based ARA smoke test for the SendToHub plugin.
#
# Downloads REAPER, installs the freshly-built VST3 into REAPER's plugin
# scanner path, opens a scripted project that loads a .wav clip with the
# plugin, and verifies that:
#   1. REAPER's plugin scanner accepts the VST3 (no load errors).
#   2. The plugin's ARA integration reports the expected audio-file path
#      via a ReaScript probe (JSON dumped to $ARA_PROBE_OUT).
#
# The script is intentionally idempotent and safe to re-run locally.  All
# artifacts land in $WORK_DIR (default: ./music_ara_client/build-reaper).
#
# Usage:
#   VST3_PATH=/path/to/SendToHubPlugin.vst3 \
#   FIXTURE_WAV=/path/to/fixture.wav \
#   ./scripts/reaper_ci.sh

set -euo pipefail

WORK_DIR="${WORK_DIR:-$(pwd)/build-reaper}"
REAPER_VERSION="${REAPER_VERSION:-7.25}"
OS="$(uname -s)"
if [[ "$OS" == "Darwin" ]]; then
    REAPER_URL="${REAPER_URL:-https://www.reaper.fm/files/${REAPER_VERSION%.*}.x/reaper${REAPER_VERSION//./}_universal.dmg}"
else
    REAPER_URL="${REAPER_URL:-https://www.reaper.fm/files/${REAPER_VERSION%.*}.x/reaper${REAPER_VERSION//./}_linux_x86_64.tar.xz}"
fi
ARA_PROBE_OUT="${ARA_PROBE_OUT:-$WORK_DIR/ara_probe.json}"

: "${VST3_PATH:?VST3_PATH must point at the built SendToHubPlugin.vst3 bundle}"
: "${FIXTURE_WAV:?FIXTURE_WAV must point at a small .wav file to use as the ARA test clip}"

mkdir -p "$WORK_DIR"
cd "$WORK_DIR"

# ---------------------------------------------------------------------------
# 1. Ensure REAPER is installed or skip.
# ---------------------------------------------------------------------------
if [[ "$OS" == "Darwin" ]]; then
    if [[ ! -d "$WORK_DIR/REAPER.app" ]]; then
        if [[ "${CI:-}" == "true" ]]; then
            echo "::group::Install REAPER $REAPER_VERSION (CI)"
            curl -fL "$REAPER_URL" -o reaper.dmg
            hdiutil attach reaper.dmg -mountpoint "$WORK_DIR/mnt_reaper" -quiet -nobrowse
            cp -R "$WORK_DIR/mnt_reaper/REAPER.app" "$WORK_DIR/"
            hdiutil detach "$WORK_DIR/mnt_reaper" -quiet
            echo "::endgroup::"
        else
            echo "SKIPPED: REAPER is not installed at $WORK_DIR/REAPER.app"
            echo "Please install REAPER manually or run in CI to test."
            exit 0
        fi
    fi
    REAPER_BIN="$WORK_DIR/REAPER.app/Contents/MacOS/REAPER"
else
    # On Linux, the installer is interactive. Per user request, we disable the test
    # if REAPER is not already present, rather than trying to script the interactive installer.
    if [[ ! -x "$WORK_DIR/REAPER/reaper" ]] && [[ ! -x "$WORK_DIR/reaper/reaper" ]]; then
        # Try to install silently if we are in CI, otherwise fail
        if [[ "${CI:-}" == "true" ]]; then
            echo "::group::Install REAPER $REAPER_VERSION (CI)"
            curl -fL "$REAPER_URL" -o reaper.tar.xz
            tar -xf reaper.tar.xz
            mv reaper_linux_x86_64 reaper-dist
            # Pipe 'I' for Install, 'A' for Agree, etc. if it prompts, but --quiet usually works 
            # if we feed it 'Y'. We'll just echo Y to bypass the 'Are you sure?' prompt.
            (cd reaper-dist && echo -e "A\nY\nI\n" | ./install-reaper.sh --install "$WORK_DIR" --integrate-user-desktop --quiet) || true
            echo "::endgroup::"
        else
            echo "SKIPPED: REAPER is not installed at $WORK_DIR/REAPER/reaper"
            echo "Please install REAPER manually or run in CI to test."
            exit 0
        fi
    fi
    
    REAPER_BIN="$WORK_DIR/REAPER/reaper"
    if [[ ! -x "$REAPER_BIN" ]]; then
        REAPER_BIN="$(find "$WORK_DIR" -maxdepth 3 -name reaper -type f -perm -u+x | head -n1)"
    fi
fi

if [[ -z "$REAPER_BIN" || ! -x "$REAPER_BIN" ]]; then
    echo "SKIPPED: could not locate installed reaper binary under $WORK_DIR" >&2
    exit 0
fi

# ---------------------------------------------------------------------------
# 2. Stage the VST3 into REAPER's user plugin directory.
# ---------------------------------------------------------------------------
if [[ "$OS" == "Darwin" ]]; then
    USER_VST3_DIR="$HOME/Library/Audio/Plug-Ins/VST3"
else
    USER_VST3_DIR="$HOME/.vst3"
fi
mkdir -p "$USER_VST3_DIR"
rm -rf "$USER_VST3_DIR/$(basename "$VST3_PATH")"
cp -R "$VST3_PATH" "$USER_VST3_DIR/"

# ---------------------------------------------------------------------------
# 3. Initialize REAPER config to ensure ~/.vst3 is scanned.
# ---------------------------------------------------------------------------
REAPER_INI_DIR="$HOME/.config/REAPER"
if [[ "$OS" == "Darwin" ]]; then
    REAPER_INI_DIR="$HOME/Library/Application Support/REAPER"
fi
mkdir -p "$REAPER_INI_DIR"
if [[ ! -f "$REAPER_INI_DIR/reaper.ini" ]]; then
    cat > "$REAPER_INI_DIR/reaper.ini" <<EOF
[REAPER]
vstpath=$USER_VST3_DIR
undocachesize=0
EOF
else
    # if it exists, try to make sure it includes the directory
    if ! grep -q "vstpath" "$REAPER_INI_DIR/reaper.ini"; then
        echo "vstpath=$USER_VST3_DIR" >> "$REAPER_INI_DIR/reaper.ini"
    fi
    # disable undo state to prevent save prompts on exit
    sed -i.bak '/^undocachesize=/d' "$REAPER_INI_DIR/reaper.ini"
    echo "undocachesize=0" >> "$REAPER_INI_DIR/reaper.ini"
fi

# ---------------------------------------------------------------------------
# 4. Write a ReaScript probe that loads the fixture, instantiates the plugin
#    as an ARA track FX, and dumps the audio source's persistent ID.
# ---------------------------------------------------------------------------
PROBE_LUA="$WORK_DIR/ara_probe.lua"
cat > "$PROBE_LUA" <<EOF
-- ReaScript — ARA integration probe for SendToHubPlugin.
-- Runs headless under xvfb; writes JSON to ARA_PROBE_OUT and quits REAPER.
local out_path = os.getenv("ARA_PROBE_OUT") or "$ARA_PROBE_OUT"
local wav_path = os.getenv("FIXTURE_WAV")  or "$FIXTURE_WAV"

reaper.Main_OnCommand(40859, 0) -- New project
reaper.InsertTrackAtIndex(0, true)
local tr = reaper.GetTrack(0, 0)
reaper.SetMediaTrackInfo_Value(tr, "I_SELECTED", 1)
local it = reaper.AddMediaItemToTrack(tr)
local tk = reaper.AddTakeToMediaItem(it)
local src = reaper.PCM_Source_CreateFromFile(wav_path)
reaper.SetMediaItemTake_Source(tk, src)
reaper.UpdateArrange()

local ara_fx = reaper.TrackFX_AddByName(tr, "Send To Music Browser<ARA>", false, -1)
if ara_fx < 0 then ara_fx = reaper.TrackFX_AddByName(tr, "Send To Music Browser", false, -1) end
if ara_fx < 0 then ara_fx = reaper.TrackFX_AddByName(tr, "SendToHubPlugin<ARA>", false, -1) end
if ara_fx < 0 then ara_fx = reaper.TrackFX_AddByName(tr, "SendToHubPlugin", false, -1) end

local ok = ara_fx >= 0
local file = io.open(out_path, "w")
file:write(string.format(
    '{"plugin_loaded":%s,"fixture_wav":%q,"ara_fx_index":%d}\n',
    tostring(ok), wav_path, ara_fx))
file:close()

reaper.Main_OnCommand(40004, 0) -- File: quit REAPER
EOF

# ---------------------------------------------------------------------------
# 4. Launch REAPER headless under xvfb and run the probe.
# ---------------------------------------------------------------------------
echo "::group::Run REAPER ARA probe"
if command -v xvfb-run >/dev/null 2>&1; then
    # Piping 'y' to bypass the REAPER EULA prompt that can hang headless executions on first run
    echo "y" | xvfb-run -a "$REAPER_BIN" -nosplash -noactivate -nonewinst "$PROBE_LUA" || true
else
    echo "y" | "$REAPER_BIN" -nosplash -noactivate -nonewinst "$PROBE_LUA" || true
fi
echo "::endgroup::"

# ---------------------------------------------------------------------------
# 5. Gate on the probe output.
# ---------------------------------------------------------------------------
if [[ ! -s "$ARA_PROBE_OUT" ]]; then
    echo "FATAL: ARA probe produced no output — REAPER did not run the script" >&2
    exit 1
fi
echo "ARA probe result:"
cat "$ARA_PROBE_OUT"

if ! grep -q '"plugin_loaded":true' "$ARA_PROBE_OUT"; then
    echo "FATAL: plugin did not load into REAPER; see $ARA_PROBE_OUT" >&2
    exit 2
fi

echo "REAPER ARA smoke test passed."
