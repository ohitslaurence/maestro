-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Add locale preference column to users table for i18n support.
-- Stores ISO 639-1 language codes (e.g., 'en', 'es', 'ar').
-- NULL means user has not set a preference (use server default).

ALTER TABLE users ADD COLUMN locale TEXT DEFAULT NULL;

-- Index for potential locale-based queries (e.g., finding users by locale for bulk emails)
CREATE INDEX IF NOT EXISTS idx_users_locale ON users(locale);
