-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Users and identities for authentication system

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    display_name TEXT NOT NULL,
    primary_email TEXT UNIQUE,
    avatar_url TEXT,
    email_visible INTEGER DEFAULT 1,
    is_system_admin INTEGER DEFAULT 0,
    is_support INTEGER DEFAULT 0,
    is_auditor INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_users_email ON users(primary_email);
CREATE INDEX IF NOT EXISTS idx_users_deleted ON users(deleted_at);

CREATE TABLE IF NOT EXISTS identities (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    provider_user_id TEXT NOT NULL,
    email TEXT NOT NULL,
    email_verified INTEGER DEFAULT 0,
    access_token TEXT,
    refresh_token TEXT,
    token_expires_at TEXT,
    created_at TEXT NOT NULL,
    UNIQUE(provider, provider_user_id)
);

CREATE INDEX IF NOT EXISTS idx_identities_user ON identities(user_id);
CREATE INDEX IF NOT EXISTS idx_identities_email ON identities(email);
