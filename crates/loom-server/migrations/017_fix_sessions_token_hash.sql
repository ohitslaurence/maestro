-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Add missing token_hash column to sessions table

ALTER TABLE sessions ADD COLUMN token_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_sessions_token_hash ON sessions(token_hash);
