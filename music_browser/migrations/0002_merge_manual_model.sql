-- Migration 0002: Merge manual production model with discography model
-- Adds: devices, presets, song_instruments (live config), production stages/steps,
--        song_files, samples, and expands songs with new fields.

-- ============================================================================
-- 1. Expand instruments with a type category
-- ============================================================================
ALTER TABLE instruments ADD COLUMN instrument_type TEXT NOT NULL DEFAULT 'other'
    CHECK(instrument_type IN (
        'guitar', 'bass', 'piano', 'drums', 'vocals', 'synth',
        'strings', 'brass', 'woodwind', 'percussion', 'other'
    ));

-- ============================================================================
-- 2. Devices — physical gear (pedals, synths, mics, DAW, etc.)
-- ============================================================================
CREATE TABLE IF NOT EXISTS devices (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 128),
    device_type TEXT NOT NULL DEFAULT 'pedal'
        CHECK(device_type IN ('pedal', 'synth', 'amp', 'mic', 'daw', 'controller', 'other')),
    manual_path TEXT DEFAULT '',
    notes TEXT DEFAULT ''
);

-- ============================================================================
-- 3. Device presets — enumerated preset codes for a device
-- ============================================================================
CREATE TABLE IF NOT EXISTS device_presets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    device_id INTEGER NOT NULL REFERENCES devices(id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK(length(name) <= 256),
    preset_code TEXT DEFAULT '',
    description TEXT DEFAULT ''
);

-- ============================================================================
-- 4. Expand songs: add key, bpm, original_artist, and make album optional
-- ============================================================================

-- SQLite cannot ALTER COLUMN, so we add new columns
ALTER TABLE songs ADD COLUMN key TEXT DEFAULT '';
ALTER TABLE songs ADD COLUMN bpm_lower INTEGER DEFAULT NULL;
ALTER TABLE songs ADD COLUMN bpm_upper INTEGER DEFAULT NULL;
ALTER TABLE songs ADD COLUMN original_artist TEXT DEFAULT '';
ALTER TABLE songs ADD COLUMN score_url TEXT DEFAULT '';
ALTER TABLE songs ADD COLUMN description TEXT DEFAULT '';

-- Widen song_type to include 'original' and 'practice'
-- SQLite cannot ALTER CHECK constraints, so we create a new table and migrate
CREATE TABLE songs_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL CHECK(length(title) <= 256),
    album_id INTEGER REFERENCES albums(id) ON DELETE SET NULL,
    sheet_music TEXT DEFAULT '',
    lyrics TEXT DEFAULT '',
    song_type TEXT NOT NULL DEFAULT 'song'
        CHECK(song_type IN ('song', 'cover', 'composition', 'original', 'practice')),
    key TEXT DEFAULT '',
    bpm_lower INTEGER DEFAULT NULL,
    bpm_upper INTEGER DEFAULT NULL,
    original_artist TEXT DEFAULT '',
    score_url TEXT DEFAULT '',
    description TEXT DEFAULT ''
);

INSERT INTO songs_new (id, title, album_id, sheet_music, lyrics, song_type, key, bpm_lower, bpm_upper, original_artist, score_url, description)
    SELECT id, title, album_id, sheet_music, lyrics, song_type, key, bpm_lower, bpm_upper, original_artist, score_url, description
    FROM songs;

-- Drop old FKs pointing at songs, rebuild after swap
DROP TABLE IF EXISTS song_artists;
DROP TABLE IF EXISTS cover_instruments;
DROP TABLE IF EXISTS cover_details;
DROP TABLE IF EXISTS composition_instruments;
DROP TABLE IF EXISTS composition_details;
DROP TABLE IF EXISTS recording_instruments;
DROP TABLE IF EXISTS recordings;

DROP TABLE songs;
ALTER TABLE songs_new RENAME TO songs;

-- Re-create dependent tables with updated FK references

CREATE TABLE IF NOT EXISTS song_artists (
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    artist_id INTEGER NOT NULL REFERENCES artists(id) ON DELETE CASCADE,
    PRIMARY KEY (song_id, artist_id)
);

CREATE TABLE IF NOT EXISTS cover_details (
    song_id INTEGER PRIMARY KEY REFERENCES songs(id) ON DELETE CASCADE,
    notes_image TEXT DEFAULT '',
    notes_completed BOOLEAN NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS cover_instruments (
    song_id INTEGER NOT NULL REFERENCES cover_details(song_id) ON DELETE CASCADE,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    PRIMARY KEY (song_id, instrument_id)
);

CREATE TABLE IF NOT EXISTS composition_details (
    song_id INTEGER PRIMARY KEY REFERENCES songs(id) ON DELETE CASCADE,
    beats_per_minute_upper INTEGER NOT NULL DEFAULT 120,
    beats_per_minute_lower INTEGER NOT NULL DEFAULT 120
);

CREATE TABLE IF NOT EXISTS composition_instruments (
    song_id INTEGER NOT NULL REFERENCES composition_details(song_id) ON DELETE CASCADE,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    PRIMARY KEY (song_id, instrument_id)
);

CREATE TABLE IF NOT EXISTS recordings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    recording_type TEXT NOT NULL CHECK(recording_type IN ('audacity', 'mix', 'master', 'loop-core-list', 'wav', 'daw-project', 'practice')),
    path TEXT DEFAULT '',
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE RESTRICT,
    notes_image TEXT DEFAULT ''
);

CREATE TABLE IF NOT EXISTS recording_instruments (
    recording_id INTEGER NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    PRIMARY KEY (recording_id, instrument_id)
);

-- ============================================================================
-- 5. Song instruments — the normalized "live config" table
--    Replaces separate Guitar Cover / Bass Cover / Piano Cover tables
--    Links a song + instrument + device presets for live performance
-- ============================================================================
CREATE TABLE IF NOT EXISTS song_instruments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    description TEXT DEFAULT '',
    score_url TEXT DEFAULT '',
    production_path TEXT DEFAULT '',
    mastering_path TEXT DEFAULT ''
);

-- M2M: which device presets are used for this song+instrument combo
CREATE TABLE IF NOT EXISTS song_instrument_presets (
    song_instrument_id INTEGER NOT NULL REFERENCES song_instruments(id) ON DELETE CASCADE,
    device_preset_id INTEGER NOT NULL REFERENCES device_presets(id) ON DELETE CASCADE,
    notes TEXT DEFAULT '',
    PRIMARY KEY (song_instrument_id, device_preset_id)
);

-- ============================================================================
-- 6. Production stages and steps
-- ============================================================================
CREATE TABLE IF NOT EXISTS production_stages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    stage TEXT NOT NULL CHECK(stage IN (
        'writing', 'composition', 'tracking', 'mixing',
        'mastering', 'publishing', 'performing'
    )),
    status TEXT NOT NULL DEFAULT 'not_started' CHECK(status IN (
        'not_started', 'skipped', 'in_progress',
        'nearing_completion', 'borked', 'complete', 'exceptional'
    )),
    UNIQUE(song_id, stage)
);

CREATE TABLE IF NOT EXISTS production_steps (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    stage_id INTEGER NOT NULL REFERENCES production_stages(id) ON DELETE CASCADE,
    instrument_id INTEGER REFERENCES instruments(id) ON DELETE SET NULL,
    name TEXT NOT NULL CHECK(length(name) <= 256),
    status TEXT NOT NULL DEFAULT 'not_started' CHECK(status IN (
        'not_started', 'skipped', 'in_progress',
        'nearing_completion', 'borked', 'complete', 'exceptional'
    )),
    sort_order INTEGER NOT NULL DEFAULT 0,
    notes TEXT DEFAULT ''
);

-- ============================================================================
-- 7. Song files — DAW projects, notes, chord sheets, exports, etc.
-- ============================================================================
CREATE TABLE IF NOT EXISTS song_files (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    file_type TEXT NOT NULL CHECK(file_type IN (
        'daw_project', 'sheet_music', 'chord_sheet', 'lyrics',
        'notes_text', 'notes_pdf', 'notes_image', 'audio_export',
        'video', 'backing_track', 'lead_sheet', 'demo', 'other'
    )),
    path TEXT NOT NULL,
    instrument_id INTEGER REFERENCES instruments(id) ON DELETE SET NULL,
    description TEXT DEFAULT ''
);

-- ============================================================================
-- 8. Samples — sounds not attributed to a song
-- ============================================================================
CREATE TABLE IF NOT EXISTS samples (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 256),
    path TEXT DEFAULT '',
    bpm INTEGER DEFAULT NULL,
    key TEXT DEFAULT '',
    description TEXT DEFAULT ''
);

CREATE TABLE IF NOT EXISTS sample_instruments (
    sample_id INTEGER NOT NULL REFERENCES samples(id) ON DELETE CASCADE,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    PRIMARY KEY (sample_id, instrument_id)
);
