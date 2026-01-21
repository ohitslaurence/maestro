-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- WireGuard Tunnel System Tables
-- See specs/wgtunnel-system.md for design documentation

-- User's registered WireGuard devices (persistent keys)
-- Each device has a long-lived WG keypair that persists across sessions
CREATE TABLE IF NOT EXISTS wg_devices (
    id TEXT PRIMARY KEY,                    -- UUID
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    public_key BLOB NOT NULL UNIQUE,        -- 32 bytes, WireGuard Curve25519 public key
    name TEXT,                              -- User-friendly name: "MacBook Pro", "Work Desktop"
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT,                      -- Last API activity with this device
    revoked_at TEXT                         -- Non-null if device is revoked
);

CREATE INDEX IF NOT EXISTS idx_wg_devices_user ON wg_devices(user_id);
CREATE INDEX IF NOT EXISTS idx_wg_devices_pubkey ON wg_devices(public_key);
CREATE INDEX IF NOT EXISTS idx_wg_devices_user_active ON wg_devices(user_id) WHERE revoked_at IS NULL;

-- Active weaver WireGuard registrations (ephemeral, per-pod)
-- Weavers generate a new keypair on each pod start
CREATE TABLE IF NOT EXISTS wg_weavers (
    weaver_id TEXT PRIMARY KEY,             -- References weavers table
    public_key BLOB NOT NULL,               -- 32 bytes, ephemeral WG key for this pod
    assigned_ip TEXT NOT NULL,              -- fd7a:115c:a1e0:1::xxxx (weaver subnet)
    derp_home_region INTEGER,               -- Home DERP region ID for this weaver
    endpoint TEXT,                          -- Direct UDP endpoint if discovered (host:port)
    registered_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_seen_at TEXT                       -- Last heartbeat/activity
);

CREATE INDEX IF NOT EXISTS idx_wg_weavers_pubkey ON wg_weavers(public_key);

-- Active tunnel sessions (device ↔ weaver connections)
-- Created when a user initiates a tunnel to a weaver
CREATE TABLE IF NOT EXISTS wg_sessions (
    id TEXT PRIMARY KEY,                    -- UUID
    device_id TEXT NOT NULL REFERENCES wg_devices(id) ON DELETE CASCADE,
    weaver_id TEXT NOT NULL REFERENCES wg_weavers(weaver_id) ON DELETE CASCADE,
    client_ip TEXT NOT NULL,                -- fd7a:115c:a1e0:2::yyyy (client subnet)
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_handshake_at TEXT,                 -- Last successful WireGuard handshake
    
    UNIQUE(device_id, weaver_id)            -- One session per device-weaver pair
);

CREATE INDEX IF NOT EXISTS idx_wg_sessions_device ON wg_sessions(device_id);
CREATE INDEX IF NOT EXISTS idx_wg_sessions_weaver ON wg_sessions(weaver_id);

-- IP allocation tracking for the WireGuard overlay network
-- Manages the fd7a:115c:a1e0::/48 prefix
CREATE TABLE IF NOT EXISTS wg_ip_allocations (
    ip TEXT PRIMARY KEY,                    -- fd7a:115c:a1e0::xxxx
    allocation_type TEXT NOT NULL           -- 'weaver' or 'client'
        CHECK (allocation_type IN ('weaver', 'client')),
    entity_id TEXT NOT NULL,                -- weaver_id for weavers, session_id for clients
    allocated_at TEXT NOT NULL DEFAULT (datetime('now')),
    released_at TEXT                        -- Non-null when IP is released for reuse
);

CREATE INDEX IF NOT EXISTS idx_wg_ip_alloc_entity ON wg_ip_allocations(entity_id);
CREATE INDEX IF NOT EXISTS idx_wg_ip_alloc_available ON wg_ip_allocations(allocation_type) 
    WHERE released_at IS NOT NULL;
