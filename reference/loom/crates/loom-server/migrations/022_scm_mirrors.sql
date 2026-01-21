-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Migration: SCM repository mirrors
-- Created: 2025

CREATE TABLE IF NOT EXISTS repo_mirrors (
    id TEXT PRIMARY KEY NOT NULL,
    repo_id TEXT NOT NULL,
    remote_url TEXT NOT NULL,
    credential_key TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    last_pushed_at TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_repo_mirrors_repo_id ON repo_mirrors (repo_id);

CREATE TABLE IF NOT EXISTS mirror_branch_rules (
    mirror_id TEXT NOT NULL REFERENCES repo_mirrors(id) ON DELETE CASCADE,
    pattern TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (mirror_id, pattern)
);

CREATE TABLE IF NOT EXISTS external_mirrors (
    id TEXT PRIMARY KEY NOT NULL,
    platform TEXT NOT NULL CHECK (platform IN ('github', 'gitlab')),
    external_owner TEXT NOT NULL,
    external_repo TEXT NOT NULL,
    repo_id TEXT NOT NULL,
    last_synced_at TEXT,
    last_accessed_at TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (platform, external_owner, external_repo)
);

CREATE INDEX IF NOT EXISTS idx_external_mirrors_repo_id ON external_mirrors (repo_id);
CREATE INDEX IF NOT EXISTS idx_external_mirrors_last_accessed ON external_mirrors (last_accessed_at);
