-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- GitHub App installations table
-- Stores information about each GitHub App installation (org or user)
CREATE TABLE IF NOT EXISTS github_installations (
    installation_id      INTEGER PRIMARY KEY,  -- GitHub installation ID
    account_id           INTEGER NOT NULL,     -- GitHub account ID (user/org)
    account_login        TEXT NOT NULL,        -- Account login name (e.g., "my-org")
    account_type         TEXT NOT NULL,        -- "User" or "Organization"
    app_slug             TEXT,                 -- App slug (for multi-app setups)
    repositories_selection TEXT NOT NULL,      -- "all" or "selected"
    suspended_at         TEXT,                 -- ISO8601 timestamp or NULL
    created_at           TEXT NOT NULL,        -- ISO8601 timestamp
    updated_at           TEXT NOT NULL         -- ISO8601 timestamp
);

-- Repository to installation mapping
-- Tracks which repositories have the GitHub App installed
CREATE TABLE IF NOT EXISTS github_installation_repos (
    repository_id        INTEGER PRIMARY KEY,  -- GitHub repository ID
    installation_id      INTEGER NOT NULL,     -- FK to github_installations
    owner                TEXT NOT NULL,        -- Repository owner (user or org name)
    name                 TEXT NOT NULL,        -- Repository name (without owner prefix)
    full_name            TEXT NOT NULL,        -- Full repository name ("owner/name")
    private              INTEGER NOT NULL,     -- 0 = public, 1 = private
    default_branch       TEXT,                 -- Default branch name
    created_at           TEXT NOT NULL,        -- ISO8601 timestamp
    updated_at           TEXT NOT NULL,        -- ISO8601 timestamp
    FOREIGN KEY (installation_id) REFERENCES github_installations(installation_id)
        ON DELETE CASCADE
);

-- Index for fast owner/name lookups (primary use case for finding installations)
CREATE INDEX IF NOT EXISTS idx_github_installation_repos_owner_name
    ON github_installation_repos (owner, name);

-- Index for listing repos by installation
CREATE INDEX IF NOT EXISTS idx_github_installation_repos_installation
    ON github_installation_repos (installation_id);
