-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Thread extensions for auth and sharing

ALTER TABLE threads ADD COLUMN owner_user_id TEXT REFERENCES users(id);
ALTER TABLE threads ADD COLUMN org_id TEXT REFERENCES organizations(id);
ALTER TABLE threads ADD COLUMN team_id TEXT REFERENCES teams(id);
ALTER TABLE threads ADD COLUMN visibility TEXT DEFAULT 'private';
ALTER TABLE threads ADD COLUMN is_shared_with_support INTEGER DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_threads_owner ON threads(owner_user_id);
CREATE INDEX IF NOT EXISTS idx_threads_org ON threads(org_id);
CREATE INDEX IF NOT EXISTS idx_threads_team ON threads(team_id);
CREATE INDEX IF NOT EXISTS idx_threads_visibility ON threads(visibility);

CREATE TABLE IF NOT EXISTS share_links (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    revoked_at TEXT,
    UNIQUE(thread_id)
);

CREATE INDEX IF NOT EXISTS idx_share_links_thread ON share_links(thread_id);
CREATE INDEX IF NOT EXISTS idx_share_links_hash ON share_links(token_hash);

CREATE TABLE IF NOT EXISTS support_access (
    id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL REFERENCES threads(id) ON DELETE CASCADE,
    requested_by TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    approved_by TEXT REFERENCES users(id),
    requested_at TEXT NOT NULL,
    approved_at TEXT,
    expires_at TEXT,
    revoked_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_support_access_thread ON support_access(thread_id);
CREATE INDEX IF NOT EXISTS idx_support_access_requested_by ON support_access(requested_by);
