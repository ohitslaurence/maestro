-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Sessions System Migration
-- Adds tables for app session tracking: individual sessions and hourly aggregates
-- Note: Named app_sessions to avoid conflict with auth sessions table

-- Individual app sessions (retained for 30 days)
CREATE TABLE IF NOT EXISTS app_sessions (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,

    person_id TEXT,
    distinct_id TEXT NOT NULL,

    status TEXT NOT NULL DEFAULT 'active',

    release TEXT,
    environment TEXT NOT NULL,

    error_count INTEGER NOT NULL DEFAULT 0,
    crash_count INTEGER NOT NULL DEFAULT 0,
    crashed INTEGER NOT NULL DEFAULT 0,        -- Boolean: crash_count > 0

    started_at TEXT NOT NULL,
    ended_at TEXT,
    duration_ms INTEGER,

    platform TEXT NOT NULL,
    user_agent TEXT,
    ip_address TEXT,

    sampled INTEGER NOT NULL DEFAULT 1,        -- Boolean
    sample_rate REAL NOT NULL DEFAULT 1.0,

    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_app_sessions_project_id ON app_sessions(project_id);
CREATE INDEX IF NOT EXISTS idx_app_sessions_release ON app_sessions(release);
CREATE INDEX IF NOT EXISTS idx_app_sessions_started_at ON app_sessions(started_at);
CREATE INDEX IF NOT EXISTS idx_app_sessions_person_id ON app_sessions(person_id);
CREATE INDEX IF NOT EXISTS idx_app_sessions_status ON app_sessions(status);

-- Hourly aggregates (retained forever)
CREATE TABLE IF NOT EXISTS app_session_aggregates (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,

    release TEXT,
    environment TEXT NOT NULL,
    hour TEXT NOT NULL,                        -- ISO8601 truncated to hour

    total_sessions INTEGER NOT NULL DEFAULT 0,
    exited_sessions INTEGER NOT NULL DEFAULT 0,
    crashed_sessions INTEGER NOT NULL DEFAULT 0,
    abnormal_sessions INTEGER NOT NULL DEFAULT 0,
    errored_sessions INTEGER NOT NULL DEFAULT 0,

    unique_users INTEGER NOT NULL DEFAULT 0,
    crashed_users INTEGER NOT NULL DEFAULT 0,

    total_duration_ms INTEGER NOT NULL DEFAULT 0,
    min_duration_ms INTEGER,
    max_duration_ms INTEGER,

    total_errors INTEGER NOT NULL DEFAULT 0,
    total_crashes INTEGER NOT NULL DEFAULT 0,

    updated_at TEXT NOT NULL,

    UNIQUE(project_id, release, environment, hour)
);

CREATE INDEX IF NOT EXISTS idx_app_session_aggregates_project_id ON app_session_aggregates(project_id);
CREATE INDEX IF NOT EXISTS idx_app_session_aggregates_release ON app_session_aggregates(release);
CREATE INDEX IF NOT EXISTS idx_app_session_aggregates_hour ON app_session_aggregates(hour);
CREATE INDEX IF NOT EXISTS idx_app_session_aggregates_lookup
ON app_session_aggregates(project_id, release, environment, hour);
