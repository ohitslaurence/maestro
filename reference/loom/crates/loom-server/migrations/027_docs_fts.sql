-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Full-text search table for documentation content
-- Populated at server startup from docs-index.json exported by loom-web build

CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(
    doc_id UNINDEXED,          -- stable identifier (e.g. "tutorials/getting-started")
    path UNINDEXED,            -- URL path: "/docs/tutorials/getting-started"
    title,                     -- searchable
    summary,                   -- searchable
    body,                      -- full text content
    diataxis UNINDEXED,        -- tutorial|how-to|reference|explanation
    tags,                      -- space-separated tags (searchable)
    updated_at UNINDEXED,      -- ISO8601 string
    tokenize = 'unicode61',
    prefix = '2 3'
);
