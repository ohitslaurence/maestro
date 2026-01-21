-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Cron Monitoring System Migration
-- Adds tables for job/cron monitoring: monitors, checkins, and stats

-- Monitors
CREATE TABLE IF NOT EXISTS cron_monitors (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    project_id TEXT REFERENCES crash_projects(id) ON DELETE SET NULL,

    slug TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,

    status TEXT NOT NULL DEFAULT 'active',
    health TEXT NOT NULL DEFAULT 'unknown',

    schedule_type TEXT NOT NULL,          -- 'cron' or 'interval'
    schedule_value TEXT NOT NULL,         -- Cron expression or interval minutes
    timezone TEXT NOT NULL DEFAULT 'UTC',

    checkin_margin_minutes INTEGER NOT NULL DEFAULT 5,
    max_runtime_minutes INTEGER,

    ping_key TEXT NOT NULL UNIQUE,

    environments TEXT NOT NULL DEFAULT '[]',  -- JSON array

    last_checkin_at TEXT,
    last_checkin_status TEXT,
    next_expected_at TEXT,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    total_checkins INTEGER NOT NULL DEFAULT 0,
    total_failures INTEGER NOT NULL DEFAULT 0,

    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,

    UNIQUE(org_id, slug)
);

CREATE INDEX IF NOT EXISTS idx_cron_monitors_org_id ON cron_monitors(org_id);
CREATE INDEX IF NOT EXISTS idx_cron_monitors_ping_key ON cron_monitors(ping_key);
CREATE INDEX IF NOT EXISTS idx_cron_monitors_status ON cron_monitors(status);
CREATE INDEX IF NOT EXISTS idx_cron_monitors_next_expected ON cron_monitors(next_expected_at);

-- Check-ins
CREATE TABLE IF NOT EXISTS cron_checkins (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES cron_monitors(id) ON DELETE CASCADE,

    status TEXT NOT NULL,

    started_at TEXT,
    finished_at TEXT NOT NULL,
    duration_ms INTEGER,

    environment TEXT,
    release TEXT,

    exit_code INTEGER,
    output TEXT,
    crash_event_id TEXT,

    source TEXT NOT NULL,

    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_cron_checkins_monitor_id ON cron_checkins(monitor_id);
CREATE INDEX IF NOT EXISTS idx_cron_checkins_status ON cron_checkins(status);
CREATE INDEX IF NOT EXISTS idx_cron_checkins_finished_at ON cron_checkins(finished_at);

-- Aggregated stats (daily rollups)
CREATE TABLE IF NOT EXISTS cron_monitor_stats (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES cron_monitors(id) ON DELETE CASCADE,
    date TEXT NOT NULL,                   -- "2026-01-18"

    total_checkins INTEGER NOT NULL DEFAULT 0,
    successful_checkins INTEGER NOT NULL DEFAULT 0,
    failed_checkins INTEGER NOT NULL DEFAULT 0,
    missed_checkins INTEGER NOT NULL DEFAULT 0,
    timeout_checkins INTEGER NOT NULL DEFAULT 0,

    avg_duration_ms INTEGER,
    min_duration_ms INTEGER,
    max_duration_ms INTEGER,

    updated_at TEXT NOT NULL,

    UNIQUE(monitor_id, date)
);

CREATE INDEX IF NOT EXISTS idx_cron_monitor_stats_monitor_id ON cron_monitor_stats(monitor_id);
CREATE INDEX IF NOT EXISTS idx_cron_monitor_stats_date ON cron_monitor_stats(date);
