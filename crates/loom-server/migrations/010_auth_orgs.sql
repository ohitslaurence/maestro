-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Organizations and memberships

CREATE TABLE IF NOT EXISTS organizations (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT UNIQUE NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'public',
    is_personal INTEGER DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_organizations_slug ON organizations(slug);
CREATE INDEX IF NOT EXISTS idx_organizations_deleted ON organizations(deleted_at);

CREATE TABLE IF NOT EXISTS org_memberships (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(org_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_org_memberships_org ON org_memberships(org_id);
CREATE INDEX IF NOT EXISTS idx_org_memberships_user ON org_memberships(user_id);

CREATE TABLE IF NOT EXISTS org_invitations (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    email TEXT NOT NULL,
    role TEXT NOT NULL,
    invited_by TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    accepted_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_org_invitations_org ON org_invitations(org_id);
CREATE INDEX IF NOT EXISTS idx_org_invitations_email ON org_invitations(email);
CREATE INDEX IF NOT EXISTS idx_org_invitations_expires ON org_invitations(expires_at);

CREATE TABLE IF NOT EXISTS org_join_requests (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    handled_at TEXT,
    handled_by TEXT REFERENCES users(id),
    approved INTEGER
);

CREATE INDEX IF NOT EXISTS idx_org_join_requests_org ON org_join_requests(org_id);
CREATE INDEX IF NOT EXISTS idx_org_join_requests_user ON org_join_requests(user_id);
