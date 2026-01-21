-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Add repository maintenance jobs table
CREATE TABLE IF NOT EXISTS repo_maintenance_jobs (
    id TEXT PRIMARY KEY NOT NULL,
    repo_id TEXT REFERENCES repos(id) ON DELETE CASCADE,
    task TEXT NOT NULL CHECK (task IN ('gc', 'prune', 'repack', 'fsck', 'all')),
    status TEXT NOT NULL CHECK (status IN ('pending', 'running', 'success', 'failed')),
    started_at TEXT,
    finished_at TEXT,
    error TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_repo_maintenance_jobs_repo ON repo_maintenance_jobs (repo_id);
CREATE INDEX IF NOT EXISTS idx_repo_maintenance_jobs_status ON repo_maintenance_jobs (status, created_at);
