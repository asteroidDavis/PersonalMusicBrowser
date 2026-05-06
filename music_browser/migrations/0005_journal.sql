-- Migration 0005: Journal — horizontal tracking of practice completions
-- Adds: journal_entries table to track completions across schedule items and goals
-- This is a horizontal feature that lives at the user profile level, not per-song

-- ============================================================================
-- 1. Journal entries — horizontal tracking of completions
-- ============================================================================
CREATE TABLE IF NOT EXISTS journal_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entry_date TEXT NOT NULL,
    entry_type TEXT NOT NULL CHECK(entry_type IN ('schedule_item', 'goal')),
    schedule_item_id INTEGER REFERENCES schedule_items(id) ON DELETE CASCADE,
    goal_id INTEGER REFERENCES goals(id) ON DELETE CASCADE,
    notes TEXT DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    CHECK(
        (entry_type = 'schedule_item' AND schedule_item_id IS NOT NULL AND goal_id IS NULL) OR
        (entry_type = 'goal' AND goal_id IS NOT NULL AND schedule_item_id IS NULL)
    )
);

-- Index for querying by date
CREATE INDEX IF NOT EXISTS idx_journal_entries_date ON journal_entries(entry_date DESC);

-- Index for querying by type
CREATE INDEX IF NOT EXISTS idx_journal_entries_type ON journal_entries(entry_type);
