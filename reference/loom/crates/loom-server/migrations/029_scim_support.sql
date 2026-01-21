-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- SCIM Support Migration
-- Adds fields for SCIM user/group provisioning

-- Add SCIM fields to users table
ALTER TABLE users ADD COLUMN scim_external_id TEXT;
ALTER TABLE users ADD COLUMN provisioned_by_scim INTEGER NOT NULL DEFAULT 0;

-- Add SCIM fields to teams table
ALTER TABLE teams ADD COLUMN scim_external_id TEXT;
ALTER TABLE teams ADD COLUMN scim_managed INTEGER NOT NULL DEFAULT 0;

-- Add provisioned_by field to org_memberships table
ALTER TABLE org_memberships ADD COLUMN provisioned_by TEXT;

-- Create indexes for efficient SCIM lookups
CREATE INDEX IF NOT EXISTS idx_users_scim_external_id ON users(scim_external_id) WHERE scim_external_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_teams_scim_external_id ON teams(scim_external_id) WHERE scim_external_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_teams_scim_managed ON teams(scim_managed) WHERE scim_managed = 1;
