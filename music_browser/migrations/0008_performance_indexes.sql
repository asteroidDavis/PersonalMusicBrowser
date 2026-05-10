-- Migration 0008: Add performance indexes for common query patterns
-- Adds: indexes on foreign keys, date fields, and frequently queried columns

-- ============================================================================
-- 1. Foreign key indexes for join performance
-- ============================================================================

-- Song foreign keys
CREATE INDEX IF NOT EXISTS idx_songs_album_id ON songs(album_id);
CREATE INDEX IF NOT EXISTS idx_songs_workflow_state ON songs(workflow_state);
CREATE INDEX IF NOT EXISTS idx_songs_song_type ON songs(song_type);
CREATE INDEX IF NOT EXISTS idx_songs_practice_priority ON songs(practice_priority);

-- Device preset foreign keys
CREATE INDEX IF NOT EXISTS idx_device_presets_device_id ON device_presets(device_id);

-- Song instrument foreign keys
CREATE INDEX IF NOT EXISTS idx_song_instruments_song_id ON song_instruments(song_id);
CREATE INDEX IF NOT EXISTS idx_song_instruments_instrument_id ON song_instruments(instrument_id);

-- Song instrument preset foreign keys
CREATE INDEX IF NOT EXISTS idx_song_instrument_presets_song_instrument_id ON song_instrument_presets(song_instrument_id);
CREATE INDEX IF NOT EXISTS idx_song_instrument_presets_device_preset_id ON song_instrument_presets(device_preset_id);

-- Production stage foreign keys
CREATE INDEX IF NOT EXISTS idx_production_stages_song_id ON production_stages(song_id);
CREATE INDEX IF NOT EXISTS idx_production_stages_status ON production_stages(status);

-- Production step foreign keys
CREATE INDEX IF NOT EXISTS idx_production_steps_stage_id ON production_steps(stage_id);
CREATE INDEX IF NOT EXISTS idx_production_steps_instrument_id ON production_steps(instrument_id);
CREATE INDEX IF NOT EXISTS idx_production_steps_status ON production_steps(status);

-- Song file foreign keys
CREATE INDEX IF NOT EXISTS idx_song_files_song_id ON song_files(song_id);
CREATE INDEX IF NOT EXISTS idx_song_files_instrument_id ON song_files(instrument_id);
CREATE INDEX IF NOT EXISTS idx_song_files_file_type ON song_files(file_type);

-- Sample instrument foreign keys
CREATE INDEX IF NOT EXISTS idx_sample_instruments_sample_id ON sample_instruments(sample_id);
CREATE INDEX IF NOT EXISTS idx_sample_instruments_instrument_id ON sample_instruments(instrument_id);

-- Song exercise foreign keys
CREATE INDEX IF NOT EXISTS idx_song_exercises_song_id ON song_exercises(song_id);
CREATE INDEX IF NOT EXISTS idx_song_exercises_exercise_id ON song_exercises(exercise_id);

-- Schedule event foreign keys
CREATE INDEX IF NOT EXISTS idx_schedule_items_event_id ON schedule_items(event_id);
CREATE INDEX IF NOT EXISTS idx_schedule_items_song_id ON schedule_items(song_id);
CREATE INDEX IF NOT EXISTS idx_schedule_items_exercise_id ON schedule_items(exercise_id);
CREATE INDEX IF NOT EXISTS idx_schedule_items_stage_id ON schedule_items(stage_id);
CREATE INDEX IF NOT EXISTS idx_schedule_items_instrument_id ON schedule_items(instrument_id);
CREATE INDEX IF NOT EXISTS idx_schedule_items_item_type ON schedule_items(item_type);
CREATE INDEX IF NOT EXISTS idx_schedule_items_status ON schedule_items(completed);

-- Live set song foreign keys
CREATE INDEX IF NOT EXISTS idx_live_set_songs_set_id ON live_set_songs(set_id);
CREATE INDEX IF NOT EXISTS idx_live_set_songs_song_id ON live_set_songs(song_id);

-- ============================================================================
-- 2. Date/time indexes for chronological queries
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_schedule_events_event_date ON schedule_events(event_date);
CREATE INDEX IF NOT EXISTS idx_schedule_events_status ON schedule_events(status);
CREATE INDEX IF NOT EXISTS idx_goals_created_at ON goals(created_at);
CREATE INDEX IF NOT EXISTS idx_goals_horizon ON goals(horizon);
CREATE INDEX IF NOT EXISTS idx_goals_completed ON goals(completed);
CREATE INDEX IF NOT EXISTS idx_journal_entries_created_at ON journal_entries(created_at);

-- ============================================================================
-- 3. Practice exercise indexes
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_practice_exercises_instrument_id ON practice_exercises(instrument_id);
CREATE INDEX IF NOT EXISTS idx_practice_exercises_category ON practice_exercises(category);
CREATE INDEX IF NOT EXISTS idx_practice_exercises_sort_order ON practice_exercises(sort_order);

-- ============================================================================
-- 4. Live set indexes
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_live_sets_set_type ON live_sets(set_type);
CREATE INDEX IF NOT EXISTS idx_live_sets_created_at ON live_sets(created_at);
CREATE INDEX IF NOT EXISTS idx_live_set_songs_sort_order ON live_set_songs(sort_order);

-- ============================================================================
-- 5. Recording indexes
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_recordings_song_id ON recordings(song_id);
CREATE INDEX IF NOT EXISTS idx_recordings_recording_type ON recordings(recording_type);

-- ============================================================================
-- 6. Artist/Band relationship indexes
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_artist_bands_band_id ON artist_bands(band_id);

-- ============================================================================
-- 7. Cover/Composition detail indexes
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_cover_details_notes_completed ON cover_details(notes_completed);
CREATE INDEX IF NOT EXISTS idx_composition_details_beats_per_minute_upper ON composition_details(beats_per_minute_upper);
CREATE INDEX IF NOT EXISTS idx_composition_details_beats_per_minute_lower ON composition_details(beats_per_minute_lower);

-- ============================================================================
-- 8. User profile index (single-row table, but indexed for consistency)
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_user_profile_id ON user_profile(id);
