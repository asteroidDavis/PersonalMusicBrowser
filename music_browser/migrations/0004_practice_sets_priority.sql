-- Migration 0004: Practice enhancements — sets, priority, practice project, time signature
--
-- Adds: practice_project_path, time_signature, practice_priority on songs;
--        live_sets and live_set_songs tables for set/album practice grouping.

-- ============================================================================
-- 1. Extend songs with practice-specific fields
-- ============================================================================

-- GarageBand/practice project path (not a full DAW — only used when not tracking/performing)
ALTER TABLE songs ADD COLUMN practice_project_path TEXT DEFAULT '';

-- Time signature (e.g. "4/4", "3/4", "6/8")
ALTER TABLE songs ADD COLUMN time_signature TEXT DEFAULT '4/4';

-- Practice priority: 1 (highest) to 5 (lowest), 0 = unranked
ALTER TABLE songs ADD COLUMN practice_priority INTEGER NOT NULL DEFAULT 0
    CHECK(practice_priority >= 0 AND practice_priority <= 5);

-- ============================================================================
-- 2. Live sets — ordered groupings of songs for live performance or album practice
-- ============================================================================
CREATE TABLE IF NOT EXISTS live_sets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 256),
    set_type TEXT NOT NULL DEFAULT 'live'
        CHECK(set_type IN ('live', 'album_practice', 'rehearsal')),
    description TEXT DEFAULT '',
    target_duration_seconds INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Songs within a set, with ordering and per-song backing track
CREATE TABLE IF NOT EXISTS live_set_songs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    set_id INTEGER NOT NULL REFERENCES live_sets(id) ON DELETE CASCADE,
    song_id INTEGER NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    backing_track_path TEXT DEFAULT '',
    duration_seconds INTEGER DEFAULT 0,
    transition_notes TEXT DEFAULT '',
    UNIQUE(set_id, song_id)
);
