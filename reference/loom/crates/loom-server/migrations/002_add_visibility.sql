-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- 002_add_visibility.sql
-- Add visibility column to threads table for access control.
-- Visibility values: 'organization', 'private', 'public'
-- Default is 'organization' - visible to organization members.

ALTER TABLE threads
ADD COLUMN visibility TEXT NOT NULL DEFAULT 'organization';

-- Add is_shared_with_support flag for support access (independent of visibility)
ALTER TABLE threads
ADD COLUMN is_shared_with_support INTEGER NOT NULL DEFAULT 0;

-- Index for filtering by visibility (future: public thread browsing)
CREATE INDEX IF NOT EXISTS idx_threads_visibility
    ON threads (visibility, last_activity_at DESC)
    WHERE deleted_at IS NULL;

-- Index for support-shared threads
CREATE INDEX IF NOT EXISTS idx_threads_is_shared_with_support
    ON threads (is_shared_with_support, last_activity_at DESC)
    WHERE deleted_at IS NULL AND is_shared_with_support = 1;
