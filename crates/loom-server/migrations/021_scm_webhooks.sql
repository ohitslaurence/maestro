-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Migration: SCM webhooks
-- Created: 2025

CREATE TABLE IF NOT EXISTS webhooks (
    id TEXT PRIMARY KEY NOT NULL,
    owner_type TEXT NOT NULL CHECK (owner_type IN ('repo', 'org')),
    owner_id TEXT NOT NULL,
    url TEXT NOT NULL,
    secret TEXT NOT NULL,
    payload_format TEXT NOT NULL CHECK (payload_format IN ('github-compat', 'loom-v1')),
    events TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_webhooks_owner ON webhooks (owner_type, owner_id);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id TEXT PRIMARY KEY NOT NULL,
    webhook_id TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event TEXT NOT NULL,
    payload TEXT NOT NULL,
    response_code INTEGER,
    response_body TEXT,
    delivered_at TEXT,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_retry_at TEXT,
    status TEXT NOT NULL CHECK (status IN ('pending', 'success', 'failed'))
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_webhook ON webhook_deliveries (webhook_id);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_status ON webhook_deliveries (status, next_retry_at);
