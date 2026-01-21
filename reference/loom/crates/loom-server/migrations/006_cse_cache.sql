-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- CSE cache table for caching Google Custom Search Engine responses
-- Responses are cached for 24 hours to reduce API calls and improve latency

CREATE TABLE IF NOT EXISTS cse_cache (
    query         TEXT    NOT NULL,
    max_results   INTEGER NOT NULL,
    response_json TEXT    NOT NULL,
    created_at    TEXT    NOT NULL, -- RFC3339 UTC timestamp
    PRIMARY KEY (query, max_results)
);

-- Index for cleanup queries on expired entries
CREATE INDEX IF NOT EXISTS idx_cse_cache_created_at
    ON cse_cache (created_at);
