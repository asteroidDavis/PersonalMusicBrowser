-- Migration 0007: Add owner_id to top-level tables for multi-tenant auth
-- Adds: owner_id (TEXT) to top-level entities to link to PocketBase users
--        and group_id (INTEGER) for group-based sharing

-- ============================================================================
-- 1. Add owner_id and group_id to top-level tables
--    owner_id: TEXT (PocketBase user ID UUID)
--    group_id: INTEGER (optional, references groups.id)
-- ============================================================================

-- Instruments
ALTER TABLE instruments ADD COLUMN owner_id TEXT DEFAULT '';
ALTER TABLE instruments ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;

-- Bands
ALTER TABLE bands ADD COLUMN owner_id TEXT DEFAULT '';
ALTER TABLE bands ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;

-- Artists
ALTER TABLE artists ADD COLUMN owner_id TEXT DEFAULT '';
ALTER TABLE artists ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;

-- Albums
ALTER TABLE albums ADD COLUMN owner_id TEXT DEFAULT '';
ALTER TABLE albums ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;

-- Devices
ALTER TABLE devices ADD COLUMN owner_id TEXT DEFAULT '';
ALTER TABLE devices ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;

-- Samples
ALTER TABLE samples ADD COLUMN owner_id TEXT DEFAULT '';
ALTER TABLE samples ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;

-- Practice exercises
ALTER TABLE practice_exercises ADD COLUMN owner_id TEXT DEFAULT '';
ALTER TABLE practice_exercises ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;

-- Goals
ALTER TABLE goals ADD COLUMN owner_id TEXT DEFAULT '';
ALTER TABLE goals ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;

-- Live sets
ALTER TABLE live_sets ADD COLUMN owner_id TEXT DEFAULT '';
ALTER TABLE live_sets ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;

-- ============================================================================
-- 2. Add owner_id to songs (child entities inherit from parent via cascade)
-- ============================================================================
ALTER TABLE songs ADD COLUMN owner_id TEXT DEFAULT '';
ALTER TABLE songs ADD COLUMN group_id INTEGER REFERENCES groups(id) ON DELETE SET NULL;

-- ============================================================================
-- 3. Add indexes for owner_id and group_id queries
-- ============================================================================

-- Instruments
CREATE INDEX IF NOT EXISTS idx_instruments_owner_id ON instruments(owner_id);
CREATE INDEX IF NOT EXISTS idx_instruments_group_id ON instruments(group_id);

-- Bands
CREATE INDEX IF NOT EXISTS idx_bands_owner_id ON bands(owner_id);
CREATE INDEX IF NOT EXISTS idx_bands_group_id ON bands(group_id);

-- Artists
CREATE INDEX IF NOT EXISTS idx_artists_owner_id ON artists(owner_id);
CREATE INDEX IF NOT EXISTS idx_artists_group_id ON artists(group_id);

-- Albums
CREATE INDEX IF NOT EXISTS idx_albums_owner_id ON albums(owner_id);
CREATE INDEX IF NOT EXISTS idx_albums_group_id ON albums(group_id);

-- Songs
CREATE INDEX IF NOT EXISTS idx_songs_owner_id ON songs(owner_id);
CREATE INDEX IF NOT EXISTS idx_songs_group_id ON songs(group_id);

-- Devices
CREATE INDEX IF NOT EXISTS idx_devices_owner_id ON devices(owner_id);
CREATE INDEX IF NOT EXISTS idx_devices_group_id ON devices(group_id);

-- Samples
CREATE INDEX IF NOT EXISTS idx_samples_owner_id ON samples(owner_id);
CREATE INDEX IF NOT EXISTS idx_samples_group_id ON samples(group_id);

-- Practice exercises
CREATE INDEX IF NOT EXISTS idx_practice_exercises_owner_id ON practice_exercises(owner_id);
CREATE INDEX IF NOT EXISTS idx_practice_exercises_group_id ON practice_exercises(group_id);

-- Goals
CREATE INDEX IF NOT EXISTS idx_goals_owner_id ON goals(owner_id);
CREATE INDEX IF NOT EXISTS idx_goals_group_id ON goals(group_id);

-- Live sets
CREATE INDEX IF NOT EXISTS idx_live_sets_owner_id ON live_sets(owner_id);
CREATE INDEX IF NOT EXISTS idx_live_sets_group_id ON live_sets(group_id);
