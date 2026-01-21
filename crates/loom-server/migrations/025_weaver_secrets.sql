-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Weaver Secrets System
-- Provides encrypted secret storage for weavers with envelope encryption

-- Encrypted Data Encryption Keys (envelope encryption)
CREATE TABLE IF NOT EXISTS encrypted_deks (
    id              TEXT PRIMARY KEY,
    encrypted_key   BLOB NOT NULL CHECK(length(encrypted_key) >= 32),
    nonce           BLOB NOT NULL CHECK(length(nonce) = 12),
    kek_version     INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Secrets metadata
CREATE TABLE IF NOT EXISTS secrets (
    id              TEXT PRIMARY KEY,
    org_id          TEXT NOT NULL,
    scope           TEXT NOT NULL CHECK (scope IN ('org', 'repo', 'weaver')),
    repo_id         TEXT,
    weaver_id       TEXT,
    name            TEXT NOT NULL,
    description     TEXT,
    current_version INTEGER NOT NULL DEFAULT 1,
    created_by      TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
    deleted_at      TEXT,

    -- Unique constraint: one secret per name per scope
    CONSTRAINT unique_secret_name UNIQUE (org_id, scope, repo_id, weaver_id, name)
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_secrets_org ON secrets(org_id) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_secrets_repo ON secrets(repo_id) WHERE deleted_at IS NULL AND repo_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_secrets_weaver ON secrets(weaver_id) WHERE deleted_at IS NULL AND weaver_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_secrets_scope ON secrets(org_id, scope) WHERE deleted_at IS NULL;

-- Secret versions (encrypted values)
CREATE TABLE IF NOT EXISTS secret_versions (
    id              TEXT PRIMARY KEY,
    secret_id       TEXT NOT NULL REFERENCES secrets(id) ON DELETE RESTRICT,
    version         INTEGER NOT NULL,
    ciphertext      BLOB NOT NULL CHECK(length(ciphertext) >= 16),
    nonce           BLOB NOT NULL CHECK(length(nonce) = 12),
    dek_id          TEXT NOT NULL REFERENCES encrypted_deks(id) ON DELETE RESTRICT,
    created_by      TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at      TEXT,
    disabled_at     TEXT,

    CONSTRAINT unique_version UNIQUE (secret_id, version)
);

CREATE INDEX IF NOT EXISTS idx_secret_versions_secret ON secret_versions(secret_id, version);
CREATE INDEX IF NOT EXISTS idx_secret_versions_dek ON secret_versions(dek_id);

-- Weaver SVID issuance log (for audit and debugging)
CREATE TABLE IF NOT EXISTS weaver_svids (
    id              TEXT PRIMARY KEY,
    weaver_id       TEXT NOT NULL,
    pod_name        TEXT NOT NULL,
    pod_namespace   TEXT NOT NULL,
    pod_uid         TEXT NOT NULL,
    org_id          TEXT NOT NULL,
    repo_id         TEXT,
    owner_user_id   TEXT NOT NULL,
    issued_at       TEXT NOT NULL DEFAULT (datetime('now')),
    expires_at      TEXT NOT NULL,
    revoked_at      TEXT
);

CREATE INDEX IF NOT EXISTS idx_weaver_svids_weaver ON weaver_svids(weaver_id);
CREATE INDEX IF NOT EXISTS idx_weaver_svids_issued ON weaver_svids(issued_at);
CREATE INDEX IF NOT EXISTS idx_weaver_svids_org ON weaver_svids(org_id);

-- Weaver secret access log (for audit, sampled)
CREATE TABLE IF NOT EXISTS weaver_secret_access (
    id              TEXT PRIMARY KEY,
    weaver_id       TEXT NOT NULL,
    secret_id       TEXT NOT NULL REFERENCES secrets(id),
    version         INTEGER NOT NULL,
    scope           TEXT NOT NULL,
    accessed_at     TEXT NOT NULL DEFAULT (datetime('now')),
    granted         INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_weaver_secret_access_weaver ON weaver_secret_access(weaver_id);
CREATE INDEX IF NOT EXISTS idx_weaver_secret_access_secret ON weaver_secret_access(secret_id);
CREATE INDEX IF NOT EXISTS idx_weaver_secret_access_time ON weaver_secret_access(accessed_at);
