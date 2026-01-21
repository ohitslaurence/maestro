-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Analytics System Migration
-- Adds tables for product analytics: persons, identities, events, merges, and API keys

-- Persons (users being tracked)
CREATE TABLE IF NOT EXISTS analytics_persons (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    properties TEXT NOT NULL DEFAULT '{}',  -- JSON object
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    merged_into_id TEXT REFERENCES analytics_persons(id),  -- For merges
    merged_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_analytics_persons_org_id ON analytics_persons(org_id);
CREATE INDEX IF NOT EXISTS idx_analytics_persons_merged_into ON analytics_persons(merged_into_id);

-- Person identities (distinct_ids linked to persons)
CREATE TABLE IF NOT EXISTS analytics_person_identities (
    id TEXT PRIMARY KEY,
    person_id TEXT NOT NULL REFERENCES analytics_persons(id) ON DELETE CASCADE,
    distinct_id TEXT NOT NULL,
    identity_type TEXT NOT NULL,  -- 'anonymous', 'identified'
    created_at TEXT NOT NULL,
    UNIQUE(person_id, distinct_id)
);

CREATE INDEX IF NOT EXISTS idx_analytics_person_identities_distinct_id ON analytics_person_identities(distinct_id);
CREATE INDEX IF NOT EXISTS idx_analytics_person_identities_person_id ON analytics_person_identities(person_id);

-- Events
CREATE TABLE IF NOT EXISTS analytics_events (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    person_id TEXT REFERENCES analytics_persons(id),
    distinct_id TEXT NOT NULL,
    event_name TEXT NOT NULL,
    properties TEXT NOT NULL DEFAULT '{}',  -- JSON object
    timestamp TEXT NOT NULL,
    ip_address TEXT,  -- Stored encrypted or hashed for privacy
    user_agent TEXT,
    lib TEXT,
    lib_version TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_analytics_events_org_id ON analytics_events(org_id);
CREATE INDEX IF NOT EXISTS idx_analytics_events_person_id ON analytics_events(person_id);
CREATE INDEX IF NOT EXISTS idx_analytics_events_distinct_id ON analytics_events(distinct_id);
CREATE INDEX IF NOT EXISTS idx_analytics_events_event_name ON analytics_events(event_name);
CREATE INDEX IF NOT EXISTS idx_analytics_events_timestamp ON analytics_events(timestamp);
-- Composite index for common query pattern: events by org and time range
CREATE INDEX IF NOT EXISTS idx_analytics_events_org_timestamp ON analytics_events(org_id, timestamp);

-- Person merges (audit trail)
CREATE TABLE IF NOT EXISTS analytics_person_merges (
    id TEXT PRIMARY KEY,
    winner_id TEXT NOT NULL REFERENCES analytics_persons(id),
    loser_id TEXT NOT NULL REFERENCES analytics_persons(id),
    reason TEXT NOT NULL,  -- JSON: { "type": "identify", "distinct_id": "...", "user_id": "..." }
    merged_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_analytics_person_merges_winner ON analytics_person_merges(winner_id);
CREATE INDEX IF NOT EXISTS idx_analytics_person_merges_loser ON analytics_person_merges(loser_id);

-- Analytics API keys
CREATE TABLE IF NOT EXISTS analytics_api_keys (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    key_type TEXT NOT NULL,  -- 'write', 'read_write'
    key_hash TEXT NOT NULL,
    created_by TEXT NOT NULL REFERENCES users(id),
    created_at TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_analytics_api_keys_org_id ON analytics_api_keys(org_id);
CREATE INDEX IF NOT EXISTS idx_analytics_api_keys_key_hash ON analytics_api_keys(key_hash);
CREATE INDEX IF NOT EXISTS idx_analytics_api_keys_revoked ON analytics_api_keys(revoked_at);
