-- Migration 0003: Fix CHECK constraints, expand file types, relax stage names,
-- add song_file_instruments M2M for multi-instrument file tracking.

-- ============================================================================
-- 1. Recreate song_files with expanded file_type CHECK
--    Adds: musicxml, midi, stem, mix, master
--    Fixes: form was sending hyphenated values that didn't match underscored CHECK
-- ============================================================================
CREATE TABLE song_files_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    file_type TEXT NOT NULL CHECK(file_type IN (
        'daw_project', 'sheet_music', 'chord_sheet', 'lyrics',
        'notes_text', 'notes_pdf', 'notes_image', 'audio_export',
        'video', 'backing_track', 'lead_sheet', 'demo', 'other',
        'musicxml', 'midi', 'stem', 'mix', 'master'
    )),
    path TEXT NOT NULL,
    instrument_id INTEGER REFERENCES instruments(id) ON DELETE SET NULL,
    description TEXT DEFAULT ''
);

INSERT INTO song_files_new (id, song_id, file_type, path, instrument_id, description)
    SELECT id, song_id, file_type, path, instrument_id, description
    FROM song_files;

DROP TABLE song_files;
ALTER TABLE song_files_new RENAME TO song_files;

-- ============================================================================
-- 2. Recreate production_stages with relaxed stage CHECK (length <= 128)
--    Allows custom stage names while keeping standard ones as convention.
-- ============================================================================
CREATE TABLE production_stages_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    stage TEXT NOT NULL CHECK(length(stage) <= 128),
    status TEXT NOT NULL DEFAULT 'not_started' CHECK(status IN (
        'not_started', 'skipped', 'in_progress',
        'nearing_completion', 'borked', 'complete', 'exceptional'
    )),
    UNIQUE(song_id, stage)
);

INSERT INTO production_stages_new (id, song_id, stage, status)
    SELECT id, song_id, stage, status
    FROM production_stages;

-- Steps reference stages; need to drop/recreate to keep FK integrity
CREATE TABLE production_steps_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    stage_id INTEGER NOT NULL REFERENCES production_stages_new(id) ON DELETE CASCADE,
    instrument_id INTEGER REFERENCES instruments(id) ON DELETE SET NULL,
    name TEXT NOT NULL CHECK(length(name) <= 256),
    status TEXT NOT NULL DEFAULT 'not_started' CHECK(status IN (
        'not_started', 'skipped', 'in_progress',
        'nearing_completion', 'borked', 'complete', 'exceptional'
    )),
    sort_order INTEGER NOT NULL DEFAULT 0,
    notes TEXT DEFAULT ''
);

INSERT INTO production_steps_new (id, stage_id, instrument_id, name, status, sort_order, notes)
    SELECT id, stage_id, instrument_id, name, status, sort_order, notes
    FROM production_steps;

DROP TABLE production_steps;
DROP TABLE production_stages;
ALTER TABLE production_stages_new RENAME TO production_stages;
ALTER TABLE production_steps_new RENAME TO production_steps;

-- ============================================================================
-- 3. Song file instruments — M2M for files that span multiple instruments
-- ============================================================================
CREATE TABLE IF NOT EXISTS song_file_instruments (
    song_file_id INTEGER NOT NULL REFERENCES song_files(id) ON DELETE CASCADE,
    instrument_id INTEGER NOT NULL REFERENCES instruments(id) ON DELETE CASCADE,
    PRIMARY KEY (song_file_id, instrument_id)
);
