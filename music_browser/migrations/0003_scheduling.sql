-- Migration 0003: Scheduling, workflow state machine, practice exercises, goals, user profile
-- Adds: workflow_state to songs, practice_exercises, song_exercises, user_profile,
--        goals, schedule_events, schedule_items, folder/musicxml fields on songs.

-- ============================================================================
-- 1. Expand songs with workflow state, folder paths, and musicxml
-- ============================================================================

-- Song workflow state machine:
--   discovered  → learning → shaky → performing → producing | cover_recording → complete
ALTER TABLE songs ADD COLUMN workflow_state TEXT NOT NULL DEFAULT 'discovered'
    CHECK(workflow_state IN (
        'discovered', 'learning', 'shaky', 'performing',
        'producing', 'cover_recording', 'complete'
    ));

ALTER TABLE songs ADD COLUMN scores_folder TEXT DEFAULT '';
ALTER TABLE songs ADD COLUMN export_folder TEXT DEFAULT '';
ALTER TABLE songs ADD COLUMN musicxml_path TEXT DEFAULT '';

-- ============================================================================
-- 2. Practice exercises — technique drills per instrument
-- ============================================================================
CREATE TABLE IF NOT EXISTS practice_exercises (
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

-- Link exercises to songs as warmups
CREATE TABLE IF NOT EXISTS song_exercises (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    exercise_id INTEGER NOT NULL REFERENCES practice_exercises(id) ON DELETE CASCADE,
    notes TEXT DEFAULT '',
    UNIQUE(song_id, exercise_id)
);

-- ============================================================================
-- 3. User profile — capacity and preferences
-- ============================================================================
CREATE TABLE IF NOT EXISTS user_profile (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    display_name TEXT NOT NULL DEFAULT 'Musician',
    songs_capacity INTEGER NOT NULL DEFAULT 3,
    warmup_minutes INTEGER NOT NULL DEFAULT 15,
    drill_minutes INTEGER NOT NULL DEFAULT 15,
    song_minutes INTEGER NOT NULL DEFAULT 30,
    review_minutes INTEGER NOT NULL DEFAULT 10,
    notes TEXT DEFAULT ''
);

INSERT OR IGNORE INTO user_profile (id) VALUES (1);

-- ============================================================================
-- 4. Goals — hierarchical planning
-- ============================================================================
CREATE TABLE IF NOT EXISTS goals (
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

-- ============================================================================
-- 5. Schedule events — generated practice/production sessions
-- ============================================================================
CREATE TABLE IF NOT EXISTS schedule_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_date TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT 'Practice Session',
    event_type TEXT NOT NULL DEFAULT 'practice'
        CHECK(event_type IN ('practice', 'production', 'mixed')),
    status TEXT NOT NULL DEFAULT 'planned'
        CHECK(status IN ('planned', 'in_progress', 'completed', 'skipped')),
    notes TEXT DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS schedule_items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    event_id INTEGER NOT NULL REFERENCES schedule_events(id) ON DELETE CASCADE,
    item_type TEXT NOT NULL CHECK(item_type IN (
        'warmup', 'drill', 'song_practice', 'song_production',
        'review', 'exercise'
    )),
    song_id INTEGER REFERENCES songs(id) ON DELETE SET NULL,
    exercise_id INTEGER REFERENCES practice_exercises(id) ON DELETE SET NULL,
    stage_id INTEGER REFERENCES production_stages(id) ON DELETE SET NULL,
    instrument_id INTEGER REFERENCES instruments(id) ON DELETE SET NULL,
    title TEXT NOT NULL DEFAULT '',
    duration_minutes INTEGER NOT NULL DEFAULT 15,
    sort_order INTEGER NOT NULL DEFAULT 0,
    completed BOOLEAN NOT NULL DEFAULT 0,
    notes TEXT DEFAULT ''
);

-- ============================================================================
-- 6. Widen production_stages stage CHECK to allow user-defined stages
--    The original CHECK was too restrictive for the scheduling workflow
-- ============================================================================
-- SQLite cannot ALTER CHECK constraints, so recreate
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
    SELECT id, song_id, stage, status FROM production_stages;

-- Preserve steps FK by dropping and recreating
CREATE TABLE production_steps_backup AS SELECT * FROM production_steps;
DROP TABLE production_steps;
DROP TABLE production_stages;
ALTER TABLE production_stages_new RENAME TO production_stages;

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

INSERT INTO production_steps (id, stage_id, instrument_id, name, status, sort_order, notes)
    SELECT id, stage_id, instrument_id, name, status, sort_order, notes
    FROM production_steps_backup;

DROP TABLE production_steps_backup;

-- ============================================================================
-- 7. Widen song_files file_type to allow musicxml
-- ============================================================================
-- Add musicxml to the allowed file types
-- SQLite cannot alter CHECK, so recreate
CREATE TABLE song_files_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    file_type TEXT NOT NULL CHECK(file_type IN (
        'daw_project', 'sheet_music', 'chord_sheet', 'lyrics',
        'notes_text', 'notes_pdf', 'notes_image', 'audio_export',
        'video', 'backing_track', 'lead_sheet', 'demo', 'musicxml',
        'score', 'other'
    )),
    path TEXT NOT NULL,
    instrument_id INTEGER REFERENCES instruments(id) ON DELETE SET NULL,
    description TEXT DEFAULT ''
);

INSERT INTO song_files_new (id, song_id, file_type, path, instrument_id, description)
    SELECT id, song_id, file_type, path, instrument_id, description FROM song_files;

DROP TABLE song_files;
ALTER TABLE song_files_new RENAME TO song_files;
