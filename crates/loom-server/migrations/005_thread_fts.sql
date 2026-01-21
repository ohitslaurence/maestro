-- Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
-- SPDX-License-Identifier: Proprietary

-- Full-text search virtual table for threads
CREATE VIRTUAL TABLE IF NOT EXISTS thread_fts USING fts5(
    thread_id UNINDEXED,
    title,
    body,
    git_branch,
    git_remote_url,
    git_commits,
    tags
);

-- AFTER INSERT: Add to FTS
CREATE TRIGGER IF NOT EXISTS thread_fts_ai
AFTER INSERT ON threads
BEGIN
    INSERT INTO thread_fts (
        thread_id, title, body, git_branch, git_remote_url, git_commits, tags
    )
    VALUES (
        new.id,
        COALESCE(new.title, json_extract(new.metadata, '$.title'), ''),
        (SELECT COALESCE(group_concat(json_extract(m.value, '$.content'), ' '), '')
         FROM json_each(json_extract(new.conversation, '$.messages')) AS m
         WHERE json_extract(m.value, '$.content') IS NOT NULL),
        COALESCE(new.git_branch, ''),
        COALESCE(new.git_remote_url, ''),
        (SELECT COALESCE(group_concat(value, ' '), '')
         FROM json_each(json_extract(new.full_json, '$.git_commits'))),
        (SELECT COALESCE(group_concat(value, ' '), '')
         FROM json_each(json_extract(new.metadata, '$.tags')))
    );
END;

-- AFTER UPDATE: Replace in FTS
CREATE TRIGGER IF NOT EXISTS thread_fts_au
AFTER UPDATE ON threads
BEGIN
    DELETE FROM thread_fts WHERE thread_id = old.id;
    INSERT INTO thread_fts (
        thread_id, title, body, git_branch, git_remote_url, git_commits, tags
    )
    VALUES (
        new.id,
        COALESCE(new.title, json_extract(new.metadata, '$.title'), ''),
        (SELECT COALESCE(group_concat(json_extract(m.value, '$.content'), ' '), '')
         FROM json_each(json_extract(new.conversation, '$.messages')) AS m
         WHERE json_extract(m.value, '$.content') IS NOT NULL),
        COALESCE(new.git_branch, ''),
        COALESCE(new.git_remote_url, ''),
        (SELECT COALESCE(group_concat(value, ' '), '')
         FROM json_each(json_extract(new.full_json, '$.git_commits'))),
        (SELECT COALESCE(group_concat(value, ' '), '')
         FROM json_each(json_extract(new.metadata, '$.tags')))
    );
END;

-- AFTER DELETE: Remove from FTS
CREATE TRIGGER IF NOT EXISTS thread_fts_ad
AFTER DELETE ON threads
BEGIN
    DELETE FROM thread_fts WHERE thread_id = old.id;
END;
