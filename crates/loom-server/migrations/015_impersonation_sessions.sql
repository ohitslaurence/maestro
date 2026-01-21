-- Migration: 015_impersonation_sessions.sql
-- Description: Create impersonation_sessions table for tracking admin impersonation sessions.
-- Part of the auth-abac-system.md specification.

-- Impersonation sessions for admin user impersonation
CREATE TABLE IF NOT EXISTS impersonation_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    admin_user_id TEXT NOT NULL REFERENCES users(id),
    target_user_id TEXT NOT NULL REFERENCES users(id),
    reason TEXT NOT NULL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Index for looking up active sessions by admin
CREATE INDEX IF NOT EXISTS idx_impersonation_admin ON impersonation_sessions(admin_user_id, ended_at);

-- Index for looking up active sessions by target
CREATE INDEX IF NOT EXISTS idx_impersonation_target ON impersonation_sessions(target_user_id, ended_at);
