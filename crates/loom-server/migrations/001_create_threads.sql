-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Thread persistence schema
-- Uses SQLite WAL mode for multi-reader, single-writer

CREATE TABLE IF NOT EXISTS threads (
    -- Primary identifier: "T-{uuid7}"
    id TEXT PRIMARY KEY NOT NULL,
    
    -- Optimistic concurrency version
    version INTEGER NOT NULL DEFAULT 1,
    
    -- Timestamps (RFC3339 format)
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_activity_at TEXT NOT NULL,
    deleted_at TEXT,  -- NULL if active, set for soft delete
    
    -- Denormalized fields for querying
    workspace_root TEXT,
    cwd TEXT,
    loom_version TEXT,
    provider TEXT,
    model TEXT,
    
    -- Metadata (denormalized for querying)
    title TEXT,
    tags TEXT,  -- JSON array as string for LIKE queries
    is_pinned INTEGER NOT NULL DEFAULT 0,
    
    -- Message count for summaries
    message_count INTEGER NOT NULL DEFAULT 0,
    
    -- Agent state (denormalized kind for filtering)
    agent_state_kind TEXT NOT NULL,
    agent_state JSON NOT NULL,
    
    -- Conversation data
    conversation JSON NOT NULL,
    
    -- Full metadata JSON
    metadata JSON NOT NULL,
    
    -- Complete thread document for schema evolution
    full_json JSON NOT NULL
);

-- Index for listing threads by workspace, ordered by activity
CREATE INDEX IF NOT EXISTS idx_threads_workspace_activity
    ON threads (workspace_root, last_activity_at DESC)
    WHERE deleted_at IS NULL;

-- Index for filtering out deleted threads
CREATE INDEX IF NOT EXISTS idx_threads_deleted
    ON threads (deleted_at);

-- Index for pinned threads
CREATE INDEX IF NOT EXISTS idx_threads_pinned
    ON threads (is_pinned, last_activity_at DESC)
    WHERE deleted_at IS NULL;

-- Index for listing all threads by activity
CREATE INDEX IF NOT EXISTS idx_threads_activity
    ON threads (last_activity_at DESC)
    WHERE deleted_at IS NULL;
