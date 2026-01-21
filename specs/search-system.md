<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Thread Search System Specification

**Status:** Draft\
**Version:** 1.0\
**Last Updated:** 2025-01-18

---

## 1. Overview

### Purpose

The Thread Search System enables users to find threads using full-text search across thread content,
git metadata, and conversation history.

### Primary Use Cases

1. **Find thread by git commit SHA**: `loom search abc123def456`
2. **Find threads by keyword**: `loom search "authentication bug"`
3. **Find threads by branch**: `loom search "feature/xyz"`
4. **Find threads by repository**: `loom search "github.com/owner/repo"`

### Goals

- **Full-Text Search**: Use SQLite FTS5 for efficient text search
- **Commit SHA Prefix Matching**: Support partial SHA matching (7+ chars)
- **Ranked Results**: Order by relevance (BM25) with recency tiebreaker
- **Offline Fallback**: Basic local search when server unavailable

### Non-Goals

- Complex query DSL (field-scoped queries like `branch:xyz`)
- Highlighted snippets in results
- Real-time search-as-you-type

---

## 2. CLI Interface

### Command

```bash
loom search <query> [OPTIONS]
```

### Options

| Option               | Default | Description         |
| -------------------- | ------- | ------------------- |
| `--limit <n>`        | 20      | Maximum results     |
| `--offset <n>`       | 0       | Pagination offset   |
| `--workspace <path>` | current | Filter by workspace |
| `--json`             | false   | Output raw JSON     |

### Examples

```bash
# Find thread by commit SHA (partial)
loom search abc123def456

# Find threads about authentication
loom search "authentication"

# Find threads in specific repo
loom search "github.com/alice/my-app"

# Find threads on feature branch
loom search "feature/add-logging"

# JSON output for scripting
loom search abc123 --json
```

### Output Format (Text)

```
Results for "abc123def456" (3 hits):

1) T-019b2b97-fddf-7602-a3e4-1c4a295110c0
   [github.com/alice/my_app] feature/add-logging
   "Add logging to my app"
   commits: abc123def456, def456ghi789

2) T-019b2b98-fddf-7602-a3e4-1c4a295110c1
   [github.com/alice/my_app] main
   "Fix authentication bug"
   tags: authentication, bug

3) T-019b2b99-fddf-7602-a3e4-1c4a295110c2
   [github.com/bob/other-repo] develop
   "Refactor database layer"
```

### Output Format (JSON)

```json
{
  "hits": [
    {
      "summary": {
        "id": "T-019b2b97-...",
        "title": "Add logging to my app",
        "workspace_root": "/home/alice/projects/my_app",
        "git_branch": "feature/add-logging",
        "git_remote_url": "github.com/alice/my_app",
        "git_initial_commit_sha": "abc123def456...",
        "git_current_commit_sha": "xyz789012...",
        ...
      },
      "score": 0.123
    }
  ],
  "limit": 20,
  "offset": 0
}
```

---

## 3. Server API

### Endpoint

```
GET /v1/threads/search
```

### Query Parameters

| Parameter   | Type   | Required | Description                    |
| ----------- | ------ | -------- | ------------------------------ |
| `q`         | string | Yes      | Search query                   |
| `workspace` | string | No       | Filter by workspace root       |
| `limit`     | u32    | No       | Max results (default: 50)      |
| `offset`    | u32    | No       | Pagination offset (default: 0) |

### Response

```json
{
  "hits": [
    {
      "summary": { ... ThreadSummary ... },
      "score": 0.123
    }
  ],
  "limit": 50,
  "offset": 0
}
```

### Error Responses

| Status | Condition          |
| ------ | ------------------ |
| 400    | Empty query string |
| 500    | Database error     |

---

## 4. Database Schema

### FTS5 Virtual Table

```sql
-- Migration: 005_thread_fts.sql

CREATE VIRTUAL TABLE IF NOT EXISTS thread_fts USING fts5(
    thread_id UNINDEXED,
    title,
    body,
    git_branch,
    git_remote_url,
    git_commits,
    tags,
    content='',
    tokenize = 'unicode61 tokenchars ''.-/_''',
    prefix = '3 4 5 6 7 8 9 10 40'
);
```

### Indexed Fields

| Field            | Source                                      | Purpose                   |
| ---------------- | ------------------------------------------- | ------------------------- |
| `thread_id`      | `threads.id`                                | Join key (not searchable) |
| `title`          | `threads.title` or `metadata.title`         | Thread title              |
| `body`           | Flattened `conversation.messages[].content` | All message content       |
| `git_branch`     | `threads.git_branch`                        | Branch name               |
| `git_remote_url` | `threads.git_remote_url`                    | Repository slug           |
| `git_commits`    | `full_json.git_commits` (space-separated)   | All commit SHAs           |
| `tags`           | `metadata.tags` (space-separated)           | Thread tags               |

### Tokenization

- **`unicode61`**: Standard Unicode tokenizer
- **`tokenchars '.-/_'`**: Treats `feature/xyz` and `github.com/owner/repo` as single tokens
- **`prefix`**: Enables prefix matching for partial SHA search (`abc123*`)

### Triggers for Incremental Indexing

```sql
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
    INSERT INTO thread_fts (...) VALUES (...);  -- same as INSERT trigger
END;

-- AFTER DELETE: Remove from FTS
CREATE TRIGGER IF NOT EXISTS thread_fts_ad
AFTER DELETE ON threads
BEGIN
    DELETE FROM thread_fts WHERE thread_id = old.id;
END;
```

### Backfill Existing Data

```sql
INSERT INTO thread_fts (thread_id, title, body, git_branch, git_remote_url, git_commits, tags)
SELECT
    t.id,
    COALESCE(t.title, json_extract(t.metadata, '$.title'), ''),
    (SELECT COALESCE(group_concat(json_extract(m.value, '$.content'), ' '), '')
     FROM json_each(json_extract(t.conversation, '$.messages')) AS m
     WHERE json_extract(m.value, '$.content') IS NOT NULL),
    COALESCE(t.git_branch, ''),
    COALESCE(t.git_remote_url, ''),
    (SELECT COALESCE(group_concat(value, ' '), '')
     FROM json_each(json_extract(t.full_json, '$.git_commits'))),
    (SELECT COALESCE(group_concat(value, ' '), '')
     FROM json_each(json_extract(t.metadata, '$.tags')))
FROM threads AS t
WHERE t.deleted_at IS NULL;
```

---

## 5. Search Algorithm

### Query Processing

1. **Detect SHA-like queries**: If query is 7-40 hex chars with no spaces, treat as commit SHA
2. **Commit SHA search**: Use `thread_commits.commit_sha LIKE 'prefix%'` for precise matching
3. **FTS search**: Wrap query in quotes, use FTS5 MATCH with BM25 scoring
4. **Fallback**: If server unavailable, use local substring search

### Ranking

- **Primary**: BM25 score from FTS5 (lower is better)
- **Secondary**: `last_activity_at DESC` for ties

### Implementation

```rust
pub async fn search(
	&self,
	query: &str,
	workspace: Option<&str>,
	limit: u32,
	offset: u32,
) -> Result<Vec<ThreadSearchHit>, ServerError> {
	// SHA-like heuristic: hex, 7–40 chars, no spaces
	let is_sha_like = {
		let q = query.trim();
		q.len() >= 7
			&& q.len() <= 40
			&& !q.contains(char::is_whitespace)
			&& q.chars().all(|c| c.is_ascii_hexdigit())
	};

	if is_sha_like {
		let hits = self
			.search_by_commit_prefix(query, workspace, limit, offset)
			.await?;
		if !hits.is_empty() {
			return Ok(hits);
		}
	}

	self.search_fts(query, workspace, limit, offset).await
}
```

---

## 6. Local Fallback Search

When server is unavailable, perform local substring search:

```rust
pub fn search_local(
	store: &LocalThreadStore,
	query: &str,
	limit: usize,
) -> Result<Vec<ThreadSummary>, ThreadStoreError> {
	let query_lower = query.to_lowercase();
	let mut matches = Vec::new();

	for summary in store.list(1000)? {
		let thread = store.load(&summary.id)?;
		if let Some(thread) = thread {
			if matches_query(&thread, &query_lower) {
				matches.push(ThreadSummary::from(&thread));
			}
		}
	}

	// Sort by last_activity_at DESC
	matches.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));
	matches.truncate(limit);

	Ok(matches)
}

fn matches_query(thread: &Thread, query: &str) -> bool {
	// Check title
	if thread
		.metadata
		.title
		.as_ref()
		.map(|t| t.to_lowercase().contains(query))
		.unwrap_or(false)
	{
		return true;
	}

	// Check git fields
	if thread
		.git_branch
		.as_ref()
		.map(|b| b.to_lowercase().contains(query))
		.unwrap_or(false)
	{
		return true;
	}

	if thread
		.git_remote_url
		.as_ref()
		.map(|u| u.to_lowercase().contains(query))
		.unwrap_or(false)
	{
		return true;
	}

	// Check commits
	for sha in &thread.git_commits {
		if sha.to_lowercase().starts_with(query) {
			return true;
		}
	}

	// Check tags
	for tag in &thread.metadata.tags {
		if tag.to_lowercase().contains(query) {
			return true;
		}
	}

	// Check message content
	for msg in &thread.conversation.messages {
		if msg.content.to_lowercase().contains(query) {
			return true;
		}
	}

	false
}
```

---

## 7. Testing Strategy

### Property-Based Tests

```rust
proptest! {
		/// **Property: Search results include thread with matching commit SHA**
		///
		/// Why: Primary use case is finding threads by commit.
		#[test]
		fn search_finds_thread_by_commit(sha in "[0-9a-f]{40}") {
				// Create thread with commit, insert, search by prefix
				// Assert thread is in results
		}

		/// **Property: FTS index stays in sync with threads table**
		///
		/// Why: Triggers must maintain FTS correctness on all mutations.
		#[test]
		fn fts_sync_on_insert_update_delete(
				title in "[a-z ]{5,50}",
				new_title in "[a-z ]{5,50}",
		) {
				// Insert -> search finds by title
				// Update title -> search finds by new title, not old
				// Delete -> search no longer finds
		}
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_search_by_commit_sha_prefix() {
	let (repo, _dir) = create_test_repo().await;

	let mut thread = create_test_thread();
	thread.git_commits = vec!["abc123def456789012345678901234567890abcd".to_string()];
	repo.upsert(&thread, None).await.unwrap();

	let hits = repo.search("abc123def", None, 10, 0).await.unwrap();
	assert_eq!(hits.len(), 1);
	assert_eq!(hits[0].summary.id, thread.id);
}

#[tokio::test]
async fn test_search_by_title_keyword() {
	let (repo, _dir) = create_test_repo().await;

	let mut thread = create_test_thread();
	thread.metadata.title = Some("Fix authentication bug".to_string());
	repo.upsert(&thread, None).await.unwrap();

	let hits = repo.search("authentication", None, 10, 0).await.unwrap();
	assert_eq!(hits.len(), 1);
}

#[tokio::test]
async fn test_search_by_branch() {
	let (repo, _dir) = create_test_repo().await;

	let mut thread = create_test_thread();
	thread.git_branch = Some("feature/xyz".to_string());
	repo.upsert(&thread, None).await.unwrap();

	let hits = repo.search("feature/xyz", None, 10, 0).await.unwrap();
	assert_eq!(hits.len(), 1);
}
```

---

## 8. Future Considerations

### 8.1 Field-Scoped Queries

Support queries like:

- `branch:feature/xyz`
- `repo:github.com/owner/repo`
- `sha:abc123`
- `tag:authentication`

### 8.2 Highlighted Snippets

Use FTS5 `snippet()` function to show matching context in results.

### 8.3 Local SQLite Index

Add local SQLite database with FTS5 for full offline search capability.

### 8.4 Search Analytics

Track search queries to understand common patterns and improve ranking.

---

## Appendix A: Full Migration SQL

See `crates/loom-server/migrations/005_thread_fts.sql` for complete migration.

## Appendix B: CLI Help Text

```
Search threads by content, git metadata, or commit SHA

Usage: loom search <QUERY> [OPTIONS]

Arguments:
  <QUERY>  Search query (text, branch name, repo URL, or commit SHA prefix)

Options:
  -l, --limit <N>         Maximum results [default: 20]
  -o, --offset <N>        Pagination offset [default: 0]
  -w, --workspace <PATH>  Filter by workspace
      --json              Output raw JSON

Examples:
  loom search abc123def456        Find thread by commit SHA
  loom search "authentication"    Find threads about authentication
  loom search "feature/xyz"       Find threads on branch
  loom search "github.com/alice/repo"  Find threads in repo
```
