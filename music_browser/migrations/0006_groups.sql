-- Migration 0006: Groups and group memberships for multi-tenant auth
-- Adds: groups table and group_memberships table to support resource sharing

-- ============================================================================
-- 1. Groups — organizational units for sharing resources
-- ============================================================================
CREATE TABLE IF NOT EXISTS groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL CHECK(length(name) <= 256),
    description TEXT DEFAULT '',
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- ============================================================================
-- 2. Group memberships — link PocketBase users to groups
--    user_id references the PocketBase users collection ID (TEXT)
-- ============================================================================
CREATE TABLE IF NOT EXISTS group_memberships (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,  -- PocketBase user ID (UUID)
    role TEXT NOT NULL DEFAULT 'member'
        CHECK(role IN ('owner', 'admin', 'member', 'viewer')),
    joined_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(group_id, user_id)
);

-- Index for querying user's groups
CREATE INDEX IF NOT EXISTS idx_group_memberships_user_id ON group_memberships(user_id);

-- Index for querying group members
CREATE INDEX IF NOT EXISTS idx_group_memberships_group_id ON group_memberships(group_id);
