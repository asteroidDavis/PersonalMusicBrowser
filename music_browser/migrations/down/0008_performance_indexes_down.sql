-- Down migration 0008: Remove performance indexes
-- This reverses migration 0008 by dropping all added indexes

-- ============================================================================
-- 1. Foreign key indexes
-- ============================================================================

DROP INDEX IF EXISTS idx_songs_album_id;
DROP INDEX IF EXISTS idx_songs_workflow_state;
DROP INDEX IF EXISTS idx_songs_song_type;
DROP INDEX IF EXISTS idx_songs_practice_priority;

DROP INDEX IF EXISTS idx_device_presets_device_id;

DROP INDEX IF EXISTS idx_song_instruments_song_id;
DROP INDEX IF EXISTS idx_song_instruments_instrument_id;

DROP INDEX IF EXISTS idx_song_instrument_presets_song_instrument_id;
DROP INDEX IF EXISTS idx_song_instrument_presets_device_preset_id;

DROP INDEX IF EXISTS idx_production_stages_song_id;
DROP INDEX IF EXISTS idx_production_stages_status;

DROP INDEX IF EXISTS idx_production_steps_stage_id;
DROP INDEX IF EXISTS idx_production_steps_instrument_id;
DROP INDEX IF EXISTS idx_production_steps_status;

DROP INDEX IF EXISTS idx_song_files_song_id;
DROP INDEX IF EXISTS idx_song_files_instrument_id;
DROP INDEX IF EXISTS idx_song_files_file_type;

DROP INDEX IF EXISTS idx_sample_instruments_sample_id;
DROP INDEX IF EXISTS idx_sample_instruments_instrument_id;

DROP INDEX IF EXISTS idx_song_exercises_song_id;
DROP INDEX IF EXISTS idx_song_exercises_exercise_id;

DROP INDEX IF EXISTS idx_schedule_items_event_id;
DROP INDEX IF EXISTS idx_schedule_items_song_id;
DROP INDEX IF EXISTS idx_schedule_items_exercise_id;
DROP INDEX IF EXISTS idx_schedule_items_stage_id;
DROP INDEX IF EXISTS idx_schedule_items_instrument_id;
DROP INDEX IF EXISTS idx_schedule_items_item_type;
DROP INDEX IF EXISTS idx_schedule_items_status;

DROP INDEX IF EXISTS idx_live_set_songs_set_id;
DROP INDEX IF EXISTS idx_live_set_songs_song_id;

-- ============================================================================
-- 2. Date/time indexes
-- ============================================================================

DROP INDEX IF EXISTS idx_schedule_events_event_date;
DROP INDEX IF EXISTS idx_schedule_events_status;
DROP INDEX IF EXISTS idx_goals_created_at;
DROP INDEX IF EXISTS idx_goals_horizon;
DROP INDEX IF EXISTS idx_goals_completed;
DROP INDEX IF EXISTS idx_journal_entries_created_at;

-- ============================================================================
-- 3. Practice exercise indexes
-- ============================================================================

DROP INDEX IF EXISTS idx_practice_exercises_instrument_id;
DROP INDEX IF EXISTS idx_practice_exercises_category;
DROP INDEX IF EXISTS idx_practice_exercises_sort_order;

-- ============================================================================
-- 4. Live set indexes
-- ============================================================================

DROP INDEX IF EXISTS idx_live_sets_set_type;
DROP INDEX IF EXISTS idx_live_sets_created_at;
DROP INDEX IF EXISTS idx_live_set_songs_sort_order;

-- ============================================================================
-- 5. Recording indexes
-- ============================================================================

DROP INDEX IF EXISTS idx_recordings_song_id;
DROP INDEX IF EXISTS idx_recordings_recording_type;

-- ============================================================================
-- 6. Artist/Band relationship indexes
-- ============================================================================

DROP INDEX IF EXISTS idx_artist_bands_band_id;

-- ============================================================================
-- 7. Cover/Composition detail indexes
-- ============================================================================

DROP INDEX IF EXISTS idx_cover_details_notes_completed;
DROP INDEX IF EXISTS idx_composition_details_beats_per_minute_upper;
DROP INDEX IF EXISTS idx_composition_details_beats_per_minute_lower;

-- ============================================================================
-- 8. User profile index
-- ============================================================================

DROP INDEX IF EXISTS idx_user_profile_id;
