-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Crash Analytics System Migration
-- Adds tables for crash tracking: projects, issues, events, symbols, releases, and API keys

-- Crash projects
CREATE TABLE IF NOT EXISTS crash_projects (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    slug TEXT NOT NULL,
    platform TEXT NOT NULL,
    auto_resolve_age_days INTEGER,
    fingerprint_rules TEXT NOT NULL DEFAULT '[]',  -- JSON array
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(org_id, slug)
);

CREATE INDEX IF NOT EXISTS idx_crash_projects_org_id ON crash_projects(org_id);

-- Crash API keys
CREATE TABLE IF NOT EXISTS crash_api_keys (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_type TEXT NOT NULL,  -- 'capture', 'admin'
    key_hash TEXT NOT NULL,
    rate_limit_per_minute INTEGER,
    allowed_origins TEXT NOT NULL DEFAULT '[]',  -- JSON array
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_crash_api_keys_project_id ON crash_api_keys(project_id);
CREATE INDEX IF NOT EXISTS idx_crash_api_keys_key_hash ON crash_api_keys(key_hash);

-- Crash issues (aggregated)
CREATE TABLE IF NOT EXISTS crash_issues (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,
    short_id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    title TEXT NOT NULL,
    culprit TEXT,
    metadata TEXT NOT NULL,  -- JSON: IssueMetadata
    status TEXT NOT NULL DEFAULT 'unresolved',
    level TEXT NOT NULL DEFAULT 'error',
    priority TEXT NOT NULL DEFAULT 'medium',
    event_count INTEGER NOT NULL DEFAULT 0,
    user_count INTEGER NOT NULL DEFAULT 0,
    first_seen TEXT NOT NULL,
    last_seen TEXT NOT NULL,
    resolved_at TEXT,
    resolved_by TEXT REFERENCES users(id),
    resolved_in_release TEXT,
    times_regressed INTEGER NOT NULL DEFAULT 0,
    last_regressed_at TEXT,
    regressed_in_release TEXT,
    assigned_to TEXT REFERENCES users(id),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(project_id, fingerprint)
);

CREATE INDEX IF NOT EXISTS idx_crash_issues_project_id ON crash_issues(project_id);
CREATE INDEX IF NOT EXISTS idx_crash_issues_status ON crash_issues(status);
CREATE INDEX IF NOT EXISTS idx_crash_issues_last_seen ON crash_issues(last_seen);
CREATE INDEX IF NOT EXISTS idx_crash_issues_fingerprint ON crash_issues(fingerprint);

-- Crash events (individual occurrences)
CREATE TABLE IF NOT EXISTS crash_events (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,
    issue_id TEXT REFERENCES crash_issues(id) ON DELETE SET NULL,
    person_id TEXT,  -- From analytics integration
    distinct_id TEXT NOT NULL,
    exception_type TEXT NOT NULL,
    exception_value TEXT NOT NULL,
    stacktrace TEXT NOT NULL,      -- JSON: Stacktrace
    raw_stacktrace TEXT,           -- JSON: Stacktrace (pre-symbolication)
    release TEXT,
    dist TEXT,
    environment TEXT NOT NULL,
    platform TEXT NOT NULL,
    runtime TEXT,                  -- JSON: Runtime
    server_name TEXT,
    tags TEXT NOT NULL DEFAULT '{}',
    extra TEXT NOT NULL DEFAULT '{}',
    user_context TEXT,             -- JSON: UserContext
    device_context TEXT,           -- JSON: DeviceContext
    browser_context TEXT,          -- JSON: BrowserContext
    os_context TEXT,               -- JSON: OsContext
    active_flags TEXT NOT NULL DEFAULT '{}',  -- JSON: flag -> variant
    request TEXT,                  -- JSON: RequestContext
    breadcrumbs TEXT NOT NULL DEFAULT '[]',   -- JSON: Breadcrumb[]
    timestamp TEXT NOT NULL,
    received_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_crash_events_project_id ON crash_events(project_id);
CREATE INDEX IF NOT EXISTS idx_crash_events_issue_id ON crash_events(issue_id);
CREATE INDEX IF NOT EXISTS idx_crash_events_timestamp ON crash_events(timestamp);
CREATE INDEX IF NOT EXISTS idx_crash_events_release ON crash_events(release);
CREATE INDEX IF NOT EXISTS idx_crash_events_person_id ON crash_events(person_id);

-- Issue-person mapping for user_count
CREATE TABLE IF NOT EXISTS crash_issue_persons (
    issue_id TEXT NOT NULL REFERENCES crash_issues(id) ON DELETE CASCADE,
    person_id TEXT NOT NULL,
    first_seen TEXT NOT NULL,
    PRIMARY KEY (issue_id, person_id)
);

-- Symbol artifacts (source maps, debug symbols)
CREATE TABLE IF NOT EXISTS symbol_artifacts (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,
    release TEXT NOT NULL,
    dist TEXT,
    artifact_type TEXT NOT NULL,  -- 'source_map', 'minified_source', 'rust_debug_info'
    name TEXT NOT NULL,
    data BLOB NOT NULL,
    size_bytes INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    source_map_url TEXT,
    sources_content INTEGER NOT NULL DEFAULT 0,  -- Boolean
    uploaded_at TEXT NOT NULL,
    uploaded_by TEXT NOT NULL REFERENCES users(id),
    last_accessed_at TEXT,
    UNIQUE(project_id, release, name, dist)
);

CREATE INDEX IF NOT EXISTS idx_symbol_artifacts_lookup
ON symbol_artifacts(project_id, release, artifact_type);
CREATE INDEX IF NOT EXISTS idx_symbol_artifacts_sha256 ON symbol_artifacts(sha256);

-- Releases
CREATE TABLE IF NOT EXISTS crash_releases (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES crash_projects(id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    short_version TEXT,
    url TEXT,
    crash_count INTEGER NOT NULL DEFAULT 0,
    new_issue_count INTEGER NOT NULL DEFAULT 0,
    regression_count INTEGER NOT NULL DEFAULT 0,
    user_count INTEGER NOT NULL DEFAULT 0,
    date_released TEXT,
    first_event TEXT,
    last_event TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(project_id, version)
);

CREATE INDEX IF NOT EXISTS idx_crash_releases_project_id ON crash_releases(project_id);
CREATE INDEX IF NOT EXISTS idx_crash_releases_version ON crash_releases(version);
