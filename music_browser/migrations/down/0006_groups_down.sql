-- Down migration 0006: Remove groups and group memberships
-- This reverses migration 0006 by dropping tables and indexes

-- ============================================================================
-- 1. Drop indexes
-- ============================================================================

DROP INDEX IF EXISTS idx_group_memberships_user_id;
DROP INDEX IF EXISTS idx_group_memberships_group_id;

-- ============================================================================
-- 2. Drop tables
-- ============================================================================

DROP TABLE IF EXISTS group_memberships;
DROP TABLE IF EXISTS groups;
