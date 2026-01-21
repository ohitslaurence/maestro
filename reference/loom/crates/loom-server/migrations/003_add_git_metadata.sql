-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Add git metadata columns to threads table
ALTER TABLE threads ADD COLUMN git_branch TEXT;
ALTER TABLE threads ADD COLUMN git_remote_url TEXT;

-- Index for finding all threads in a repository
-- Partial index excludes soft-deleted threads for efficiency
CREATE INDEX IF NOT EXISTS idx_threads_git_remote_url
    ON threads (git_remote_url)
    WHERE deleted_at IS NULL AND git_remote_url IS NOT NULL;

-- Index for finding threads by repository AND branch
-- Useful for: "show me all threads on the main branch of this repo"
CREATE INDEX IF NOT EXISTS idx_threads_git_remote_url_branch
    ON threads (git_remote_url, git_branch)
    WHERE deleted_at IS NULL AND git_remote_url IS NOT NULL;
