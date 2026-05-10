-- Down migration 0007: Remove owner_id and group_id from top-level tables
-- This reverses migration 0007 by dropping columns and indexes

-- ============================================================================
-- 1. Drop owner_id and group_id indexes
-- ============================================================================

DROP INDEX IF EXISTS idx_instruments_owner_id;
DROP INDEX IF EXISTS idx_instruments_group_id;

DROP INDEX IF EXISTS idx_bands_owner_id;
DROP INDEX IF EXISTS idx_bands_group_id;

DROP INDEX IF EXISTS idx_artists_owner_id;
DROP INDEX IF EXISTS idx_artists_group_id;

DROP INDEX IF EXISTS idx_albums_owner_id;
DROP INDEX IF EXISTS idx_albums_group_id;

DROP INDEX IF EXISTS idx_songs_owner_id;
DROP INDEX IF EXISTS idx_songs_group_id;

DROP INDEX IF EXISTS idx_devices_owner_id;
DROP INDEX IF EXISTS idx_devices_group_id;

DROP INDEX IF EXISTS idx_samples_owner_id;
DROP INDEX IF EXISTS idx_samples_group_id;

DROP INDEX IF EXISTS idx_practice_exercises_owner_id;
DROP INDEX IF EXISTS idx_practice_exercises_group_id;

DROP INDEX IF EXISTS idx_goals_owner_id;
DROP INDEX IF EXISTS idx_goals_group_id;

DROP INDEX IF EXISTS idx_live_sets_owner_id;
DROP INDEX IF EXISTS idx_live_sets_group_id;

-- ============================================================================
-- 2. Drop owner_id and group_id columns (SQLite requires recreating tables)
-- ============================================================================

-- Instruments
CREATE TABLE instruments_backup AS SELECT id, name, instrument_type FROM instruments;
DROP TABLE instruments;
CREATE TABLE instruments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 64),
    instrument_type TEXT NOT NULL DEFAULT 'other'
        CHECK(instrument_type IN (
            'guitar', 'bass', 'piano', 'drums', 'vocals', 'synth',
            'strings', 'brass', 'woodwind', 'percussion', 'other'
        ))
);
INSERT INTO instruments (id, name, instrument_type)
    SELECT id, name, instrument_type FROM instruments_backup;
DROP TABLE instruments_backup;

-- Bands
CREATE TABLE bands_backup AS SELECT id, name FROM bands;
DROP TABLE bands;
CREATE TABLE bands (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 128)
);
INSERT INTO bands (id, name)
    SELECT id, name FROM bands_backup;
DROP TABLE bands_backup;

-- Artists
CREATE TABLE artists_backup AS SELECT id, name FROM artists;
DROP TABLE artists;
CREATE TABLE artists (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 128)
);
INSERT INTO artists (id, name)
    SELECT id, name FROM artists_backup;
DROP TABLE artists_backup;

-- Albums
CREATE TABLE albums_backup AS SELECT id, title, released, url FROM albums;
DROP TABLE albums;
CREATE TABLE albums (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL CHECK(length(title) <= 256),
    released BOOLEAN NOT NULL DEFAULT 0,
    url TEXT DEFAULT ''
);
INSERT INTO albums (id, title, released, url)
    SELECT id, title, released, url FROM albums_backup;
DROP TABLE albums_backup;

-- Devices
CREATE TABLE devices_backup AS SELECT id, name, device_type, manual_path, notes FROM devices;
DROP TABLE devices;
CREATE TABLE devices (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 128),
    device_type TEXT NOT NULL DEFAULT 'pedal'
        CHECK(device_type IN ('pedal', 'synth', 'amp', 'mic', 'daw', 'controller', 'other')),
    manual_path TEXT DEFAULT '',
    notes TEXT DEFAULT ''
);
INSERT INTO devices (id, name, device_type, manual_path, notes)
    SELECT id, name, device_type, manual_path, notes FROM devices_backup;
DROP TABLE devices_backup;

-- Samples
CREATE TABLE samples_backup AS SELECT id, name, path, bpm, key, description FROM samples;
DROP TABLE samples;
CREATE TABLE samples (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 256),
    path TEXT DEFAULT '',
    bpm INTEGER DEFAULT NULL,
    key TEXT DEFAULT '',
    description TEXT DEFAULT ''
);
INSERT INTO samples (id, name, path, bpm, key, description)
    SELECT id, name, path, bpm, key, description FROM samples_backup;
DROP TABLE samples_backup;

-- Practice exercises
CREATE TABLE practice_exercises_backup AS SELECT id, instrument_id, name, category, description, source, sort_order FROM practice_exercises;
DROP TABLE practice_exercises;
CREATE TABLE practice_exercises (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    instrument_id INTEGER REFERENCES instruments(id) ON DELETE SET NULL,
    name TEXT NOT NULL CHECK(length(name) <= 256),
    category TEXT NOT NULL DEFAULT 'technique'
        CHECK(category IN (
            'technique', 'scales', 'arpeggios', 'rhythm',
            'sight_reading', 'ear_training', 'song_practice', 'other'
        )),
    description TEXT DEFAULT '',
    source TEXT DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0
);
INSERT INTO practice_exercises (id, instrument_id, name, category, description, source, sort_order)
    SELECT id, instrument_id, name, category, description, source, sort_order FROM practice_exercises_backup;
DROP TABLE practice_exercises_backup;

-- Goals
CREATE TABLE goals_backup AS SELECT id, horizon, category, title, description, target_date, completed, created_at, sort_order FROM goals;
DROP TABLE goals;
CREATE TABLE goals (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    horizon TEXT NOT NULL CHECK(horizon IN ('5_year', '1_year', '6_week', '1_week')),
    category TEXT NOT NULL DEFAULT 'general'
        CHECK(category IN ('production', 'practice', 'general')),
    title TEXT NOT NULL CHECK(length(title) <= 256),
    description TEXT DEFAULT '',
    target_date TEXT DEFAULT '',
    completed BOOLEAN NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    sort_order INTEGER NOT NULL DEFAULT 0
);
INSERT INTO goals (id, horizon, category, title, description, target_date, completed, created_at, sort_order)
    SELECT id, horizon, category, title, description, target_date, completed, created_at, sort_order FROM goals_backup;
DROP TABLE goals_backup;

-- Live sets
CREATE TABLE live_sets_backup AS SELECT id, name, set_type, description, target_duration_seconds, created_at FROM live_sets;
DROP TABLE live_sets;
CREATE TABLE live_sets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 256),
    set_type TEXT NOT NULL DEFAULT 'live'
        CHECK(set_type IN ('live', 'album_practice', 'rehearsal')),
    description TEXT DEFAULT '',
    target_duration_seconds INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO live_sets (id, name, set_type, description, target_duration_seconds, created_at)
    SELECT id, name, set_type, description, target_duration_seconds, created_at FROM live_sets_backup;
DROP TABLE live_sets_backup;

-- Songs (complex table with many columns)
CREATE TABLE songs_backup AS SELECT 
    id, title, album_id, sheet_music, lyrics, song_type, key, bpm_lower, bpm_upper, 
    original_artist, score_url, description, workflow_state, scores_folder, 
    export_folder, musicxml_path, practice_project_path, time_signature, practice_priority
FROM songs;
DROP TABLE songs;
CREATE TABLE songs (
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
    description TEXT DEFAULT '',
    workflow_state TEXT NOT NULL DEFAULT 'discovered'
        CHECK(workflow_state IN (
            'discovered', 'learning', 'shaky', 'performing',
            'producing', 'cover_recording', 'complete'
        )),
    scores_folder TEXT DEFAULT '',
    export_folder TEXT DEFAULT '',
    musicxml_path TEXT DEFAULT '',
    practice_project_path TEXT DEFAULT '',
    time_signature TEXT DEFAULT '4/4',
    practice_priority INTEGER NOT NULL DEFAULT 0
        CHECK(practice_priority >= 0 AND practice_priority <= 5)
);
INSERT INTO songs (id, title, album_id, sheet_music, lyrics, song_type, key, bpm_lower, bpm_upper, 
    original_artist, score_url, description, workflow_state, scores_folder, 
    export_folder, musicxml_path, practice_project_path, time_signature, practice_priority)
    SELECT id, title, album_id, sheet_music, lyrics, song_type, key, bpm_lower, bpm_upper, 
        original_artist, score_url, description, workflow_state, scores_folder, 
        export_folder, musicxml_path, practice_project_path, time_signature, practice_priority
    FROM songs_backup;
DROP TABLE songs_backup;
