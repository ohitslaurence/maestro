-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Add username column to users table
-- Username is unique and used for git URLs (e.g., /git/{username}/repo.git)

-- SQLite doesn't allow adding UNIQUE columns directly, so we add without constraint
-- and create a unique index instead
ALTER TABLE users ADD COLUMN username TEXT;

-- Create unique index for username (enforces uniqueness)
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username_unique ON users(username) WHERE username IS NOT NULL;

-- Note: Existing users will have NULL username initially.
-- The application will generate usernames on next login or via a backfill job.
