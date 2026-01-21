-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Thread repository table for tracking which git repo a thread was made in
CREATE TABLE IF NOT EXISTS thread_repos (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_thread_repos_slug ON thread_repos(slug);

-- Add foreign key and extended git fields to threads
ALTER TABLE threads ADD COLUMN repo_id INTEGER REFERENCES thread_repos(id);
ALTER TABLE threads ADD COLUMN git_initial_branch TEXT;
ALTER TABLE threads ADD COLUMN git_initial_commit_sha TEXT;
ALTER TABLE threads ADD COLUMN git_current_commit_sha TEXT;
ALTER TABLE threads ADD COLUMN git_start_dirty INTEGER;
ALTER TABLE threads ADD COLUMN git_end_dirty INTEGER;

-- Junction table for tracking multiple commits per thread
CREATE TABLE IF NOT EXISTS thread_commits (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id        TEXT    NOT NULL,
    repo_id          INTEGER NOT NULL,
    commit_sha       TEXT    NOT NULL,
    branch           TEXT,
    is_dirty         INTEGER NOT NULL DEFAULT 0,
    commit_message   TEXT,
    commit_timestamp INTEGER,
    observed_at      TEXT    NOT NULL,
    is_initial       INTEGER NOT NULL DEFAULT 0,
    is_final         INTEGER NOT NULL DEFAULT 0,

    UNIQUE (thread_id, commit_sha),
    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE,
    FOREIGN KEY (repo_id)   REFERENCES thread_repos(id)   ON DELETE CASCADE
);

-- Indexes for threads table
CREATE INDEX IF NOT EXISTS idx_threads_repo_id
    ON threads (repo_id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_threads_repo_branch
    ON threads (repo_id, git_branch)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_threads_initial_commit
    ON threads (git_initial_commit_sha)
    WHERE deleted_at IS NULL AND git_initial_commit_sha IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_threads_current_commit
    ON threads (git_current_commit_sha)
    WHERE deleted_at IS NULL AND git_current_commit_sha IS NOT NULL;

-- Indexes for thread_commits table
CREATE INDEX IF NOT EXISTS idx_thread_commits_thread
    ON thread_commits (thread_id);

CREATE INDEX IF NOT EXISTS idx_thread_commits_repo_commit
    ON thread_commits (repo_id, commit_sha);

CREATE INDEX IF NOT EXISTS idx_thread_commits_commit
    ON thread_commits (commit_sha);

CREATE INDEX IF NOT EXISTS idx_thread_commits_observed
    ON thread_commits (observed_at);
