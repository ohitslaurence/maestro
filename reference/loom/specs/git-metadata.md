<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Git Metadata System Specification

**Status:** Draft\
**Version:** 2.0\
**Last Updated:** 2025-01-18

---

## 1. Overview

### Purpose

The Git Metadata System adds comprehensive repository context to Loom threads, enabling:

- Tracking which git repository and branch a thread was created in
- Tracking multiple commits made during a thread session
- Time-travel queries to find threads that touched specific commits
- First-class repository entities for analytics and deduplication
- Querying threads by repository, branch, or commit

### Goals

- **Repository Context**: Capture git branch, remote URL, and commit SHAs
- **Multi-Commit Tracking**: A thread spans multiple commits during a session
- **Time-Travel**: Query threads by commit SHA or commit range
- **First-Class Repos**: Deduplicated `repos` table with foreign keys from threads
- **Query Support**: Efficient indexes for repo, branch, and commit queries
- **Non-Blocking**: Git detection failures should not affect thread creation
- **CLI-Only Detection**: Only the CLI interacts with git; server stores data passively

### Non-Goals

- Real-time branch change detection within a session
- Git operations (commit, push, etc.) from Loom
- Repository management or authentication
- Full DAG/ancestry graph storage (delegate to git for range resolution)

---

## 2. Data Model

### 2.1 Thread Git Metadata Fields

| Field                    | Type             | Description                                                |
| ------------------------ | ---------------- | ---------------------------------------------------------- |
| `git_branch`             | `Option<String>` | Current branch name (updated on each sync)                 |
| `git_remote_url`         | `Option<String>` | Normalized remote URL slug (e.g., `github.com/owner/repo`) |
| `git_initial_branch`     | `Option<String>` | Branch when thread was created                             |
| `git_initial_commit_sha` | `Option<String>` | HEAD commit SHA when thread was created                    |
| `git_current_commit_sha` | `Option<String>` | Latest HEAD commit SHA                                     |
| `git_start_dirty`        | `Option<bool>`   | Whether working tree was dirty at thread creation          |
| `git_end_dirty`          | `Option<bool>`   | Whether working tree is dirty at last update               |
| `git_commits`            | `Vec<String>`    | All commit SHAs observed during session                    |

All fields are optional to support:

- Non-git directories
- Repositories without remotes (local-only)
- Detached HEAD states
- Systems without git installed

### 2.2 JSON Schema

```json
{
	"id": "T-019b2b97-fddf-7602-a3e4-1c4a295110c0",
	"version": 5,
	"created_at": "2025-01-01T12:00:00Z",
	"updated_at": "2025-01-01T12:05:00Z",
	"last_activity_at": "2025-01-01T12:05:00Z",

	"workspace_root": "/home/alice/projects/my_app",
	"cwd": "/home/alice/projects/my_app",
	"loom_version": "0.4.0",

	"git_branch": "feature/add-logging",
	"git_remote_url": "github.com/alice/my_app",
	"git_initial_branch": "feature/add-logging",
	"git_initial_commit_sha": "abc123def456...",
	"git_current_commit_sha": "789xyz012...",
	"git_start_dirty": false,
	"git_end_dirty": true,
	"git_commits": ["abc123def456...", "def456ghi789...", "789xyz012..."],

	"provider": "anthropic",
	"model": "claude-sonnet-4-20250514",

	"conversation": { "messages": [] },
	"agent_state": { "kind": "waiting_for_user_input", "retries": 0, "pending_tool_calls": [] },
	"metadata": {}
}
```

### 2.3 Rust Types

```rust
// In loom-thread/src/model.rs

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Thread {
	pub id: ThreadId,
	pub version: u64,
	pub created_at: String,
	pub updated_at: String,
	pub last_activity_at: String,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub workspace_root: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub cwd: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub loom_version: Option<String>,

	/// Current git branch name (updated on each sync)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_branch: Option<String>,

	/// Normalized remote URL slug (e.g., "github.com/owner/repo")
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_remote_url: Option<String>,

	/// Branch when the thread was created
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_initial_branch: Option<String>,

	/// Commit SHA when the thread was created
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_initial_commit_sha: Option<String>,

	/// Latest known commit SHA
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_current_commit_sha: Option<String>,

	/// Whether working tree was dirty at thread creation
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_start_dirty: Option<bool>,

	/// Whether working tree was dirty at last update
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_end_dirty: Option<bool>,

	/// All commit SHAs observed during this session (chronological order)
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub git_commits: Vec<String>,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub provider: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub model: Option<String>,

	pub visibility: ThreadVisibility,
	pub is_private: bool,
	pub is_shared_with_support: bool,

	pub conversation: ConversationSnapshot,
	pub agent_state: AgentStateSnapshot,
	pub metadata: ThreadMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThreadSummary {
	pub id: ThreadId,
	pub version: u64,
	pub created_at: String,
	pub updated_at: String,
	pub last_activity_at: String,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub title: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub workspace_root: Option<String>,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_branch: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_remote_url: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_initial_commit_sha: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub git_current_commit_sha: Option<String>,

	#[serde(skip_serializing_if = "Option::is_none")]
	pub provider: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub model: Option<String>,

	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub tags: Vec<String>,
	pub message_count: usize,
	pub is_pinned: bool,
	pub visibility: ThreadVisibility,
}
```

---

## 3. loom-git Crate

### Purpose

A dedicated crate for git operations that:

- Uses standard `git` CLI (not libgit2)
- Provides branch, remote, commit, and dirty state detection
- Normalizes remote URLs to a consistent slug format
- Handles errors gracefully

### Crate Structure

```
loom/
└── crates/
    └── loom-git/
        ├── Cargo.toml
        └── src/
            ├── lib.rs       # Re-exports
            ├── detect.rs    # Git detection functions
            ├── normalize.rs # URL normalization
            └── error.rs     # Error types
```

### Public API

```rust
use std::path::Path;

/// Commit information
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
	/// Full 40-character SHA
	pub sha: String,
	/// Optional short subject line
	pub summary: Option<String>,
	/// Commit timestamp (unix epoch seconds)
	pub timestamp: Option<i64>,
}

/// Full repository status
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStatus {
	/// Current branch name; None if detached HEAD
	pub branch: Option<String>,
	/// Normalized remote URL slug
	pub remote_slug: Option<String>,
	/// HEAD commit information
	pub head: Option<CommitInfo>,
	/// Whether working tree has uncommitted changes
	pub is_dirty: Option<bool>,
}

/// Simple repository metadata (backward compatible)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoMetadata {
	pub branch: Option<String>,
	pub remote_slug: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum GitError {
	#[error("not a git repository: {0}")]
	NotAGitRepo(String),
	#[error("git command failed: {cmd} {args:?}: {stderr}")]
	CommandFailed {
		cmd: &'static str,
		args: Vec<String>,
		stderr: String,
	},
	#[error("git executable not found")]
	GitNotInstalled,
	#[error("io error: {0}")]
	Io(#[from] std::io::Error),
}

/// Detect full repository status including HEAD commit and dirty state
fn detect_repo_status(path: &Path) -> Result<Option<RepoStatus>, GitError>;

/// Detect basic repository metadata (branch + remote)
fn detect_repo_metadata(path: &Path) -> Result<Option<RepoMetadata>, GitError>;

/// Get current branch name
fn current_branch(path: &Path) -> Result<Option<String>, GitError>;

/// Get default remote URL (origin, then upstream, then first remote)
fn default_remote_url(path: &Path) -> Result<Option<String>, GitError>;

/// Get HEAD commit SHA
fn head_commit_sha(path: &Path) -> Result<Option<String>, GitError>;

/// Check if working tree is dirty
fn is_dirty(path: &Path) -> Result<Option<bool>, GitError>;

/// Normalize a git remote URL to "host/path" slug format
fn normalize_remote_url(raw: &str) -> Option<String>;
```

### Git Commands Used

| Function             | Command                                                  | Notes                            |
| -------------------- | -------------------------------------------------------- | -------------------------------- |
| `current_branch`     | `git rev-parse --abbrev-ref HEAD`                        | Returns "HEAD" if detached       |
| `default_remote_url` | `git remote get-url origin`                              | Fallback to upstream, then first |
| `head_commit_sha`    | `git rev-parse HEAD`                                     | Full 40-char SHA                 |
| `is_dirty`           | `git status --porcelain`                                 | Non-empty output = dirty         |
| `detect_repo_status` | Combines above + `git show -s --format=%H%n%ct%n%s HEAD` |                                  |

### URL Normalization

Converts various git remote URL formats to `host/path` slug:

| Input                                 | Output                  |
| ------------------------------------- | ----------------------- |
| `git@github.com:owner/repo.git`       | `github.com/owner/repo` |
| `https://github.com/owner/repo.git`   | `github.com/owner/repo` |
| `ssh://git@gitlab.com/group/repo.git` | `gitlab.com/group/repo` |

---

## 4. Server-Side Database Schema

### 4.1 repos Table (First-Class Entity)

```sql
-- Migration: 004_git_repos_and_commits.sql

-- Deduplicated repository table
CREATE TABLE IF NOT EXISTS repos (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    slug        TEXT NOT NULL UNIQUE,              -- "github.com/owner/repo"
    created_at  TEXT NOT NULL
        DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_repos_slug ON repos(slug);
```

### 4.2 threads Table Extensions

```sql
-- Add repo foreign key and commit tracking fields
ALTER TABLE threads ADD COLUMN repo_id INTEGER REFERENCES repos(id);
ALTER TABLE threads ADD COLUMN git_initial_branch TEXT;
ALTER TABLE threads ADD COLUMN git_initial_commit_sha TEXT;
ALTER TABLE threads ADD COLUMN git_current_commit_sha TEXT;
ALTER TABLE threads ADD COLUMN git_start_dirty INTEGER;  -- 0/1/NULL
ALTER TABLE threads ADD COLUMN git_end_dirty INTEGER;    -- 0/1/NULL

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_threads_repo_id
    ON threads (repo_id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_threads_repo_branch
    ON threads (repo_id, git_branch)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_threads_initial_commit
    ON threads (git_initial_commit_sha)
    WHERE deleted_at IS NULL AND git_initial_commit_sha IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_threads_current_commit
    ON threads (git_current_commit_sha)
    WHERE deleted_at IS NULL AND git_current_commit_sha IS NOT NULL;
```

### 4.3 thread_commits Junction Table

Tracks all commits observed during a thread session (many-to-many).

```sql
CREATE TABLE IF NOT EXISTS thread_commits (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    thread_id        TEXT    NOT NULL,
    repo_id          INTEGER NOT NULL,
    commit_sha       TEXT    NOT NULL,  -- full 40-char SHA
    branch           TEXT,              -- branch at observation
    is_dirty         INTEGER NOT NULL DEFAULT 0,
    commit_message   TEXT,              -- short subject (optional)
    commit_timestamp INTEGER,           -- unix seconds (optional)
    observed_at      TEXT    NOT NULL,  -- RFC3339
    is_initial       INTEGER NOT NULL DEFAULT 0,
    is_final         INTEGER NOT NULL DEFAULT 0,

    UNIQUE (thread_id, commit_sha),

    FOREIGN KEY (thread_id) REFERENCES threads(id) ON DELETE CASCADE,
    FOREIGN KEY (repo_id)   REFERENCES repos(id)   ON DELETE CASCADE
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_thread_commits_thread
    ON thread_commits (thread_id);

CREATE INDEX IF NOT EXISTS idx_thread_commits_repo_commit
    ON thread_commits (repo_id, commit_sha);

CREATE INDEX IF NOT EXISTS idx_thread_commits_commit
    ON thread_commits (commit_sha);

CREATE INDEX IF NOT EXISTS idx_thread_commits_observed
    ON thread_commits (observed_at);
```

### 4.4 Backfill Existing Data

```sql
-- Populate repos from existing threads
INSERT OR IGNORE INTO repos (slug, created_at)
SELECT DISTINCT git_remote_url, MIN(created_at)
FROM threads
WHERE git_remote_url IS NOT NULL
GROUP BY git_remote_url;

-- Set repo_id on existing threads
UPDATE threads
SET repo_id = (SELECT id FROM repos WHERE slug = threads.git_remote_url)
WHERE git_remote_url IS NOT NULL AND repo_id IS NULL;
```

---

## 5. Query Patterns

### 5.1 Find Threads by Repository

```sql
SELECT t.*
FROM threads t
JOIN repos r ON t.repo_id = r.id
WHERE r.slug = :slug
  AND t.deleted_at IS NULL
ORDER BY t.last_activity_at DESC
LIMIT :limit OFFSET :offset;
```

### 5.2 Find Threads by Repository + Branch

```sql
SELECT t.*
FROM threads t
JOIN repos r ON t.repo_id = r.id
WHERE r.slug = :slug
  AND t.git_branch = :branch
  AND t.deleted_at IS NULL
ORDER BY t.last_activity_at DESC;
```

### 5.3 Find Threads That Touched a Commit

```sql
SELECT DISTINCT t.*
FROM threads t
JOIN thread_commits c ON c.thread_id = t.id
WHERE c.commit_sha = :commit_sha
  AND t.deleted_at IS NULL
ORDER BY t.last_activity_at DESC;
```

### 5.4 Find Threads for Commit Range

The CLI resolves the range via `git rev-list <from>..<to>`, then queries:

```sql
SELECT DISTINCT t.*
FROM threads t
JOIN thread_commits c ON c.thread_id = t.id
JOIN repos r ON t.repo_id = r.id
WHERE r.slug = :slug
  AND c.commit_sha IN (:sha1, :sha2, :sha3, ...)
  AND t.deleted_at IS NULL;
```

### 5.5 Find Threads with Dirty Working Tree

```sql
SELECT t.*
FROM threads t
WHERE t.git_start_dirty = 1
  AND t.deleted_at IS NULL;
```

### 5.6 Find Threads Where Branch Changed

```sql
SELECT t.*
FROM threads t
WHERE t.git_initial_branch IS NOT NULL
  AND t.git_branch IS NOT NULL
  AND t.git_initial_branch != t.git_branch
  AND t.deleted_at IS NULL;
```

---

## 6. CLI Integration

### 6.1 Git State Snapshotting

The CLI snapshots git state at two points:

1. **Thread creation**: Capture initial state
2. **Thread update/sync**: Capture current state, track new commits

```rust
use loom_git::detect_repo_status;

pub fn snapshot_git_state(thread: &mut Thread, cwd: &Path) {
	let status = match detect_repo_status(cwd) {
		Ok(Some(s)) => s,
		_ => return, // not a git repo
	};

	// Remote & repo slug (set once)
	if thread.git_remote_url.is_none() {
		thread.git_remote_url = status.remote_slug.clone();
	}

	// Initial branch (set once)
	if thread.git_initial_branch.is_none() {
		thread.git_initial_branch = status.branch.clone();
	}

	// Current branch (updated each time)
	thread.git_branch = status.branch.clone();

	// Commit tracking
	let head_sha = status.head.as_ref().map(|h| h.sha.clone());

	if thread.git_initial_commit_sha.is_none() {
		thread.git_initial_commit_sha = head_sha.clone();
	}

	if let Some(sha) = &head_sha {
		thread.git_current_commit_sha = Some(sha.clone());

		// Track all commits observed during session
		if !thread.git_commits.contains(sha) {
			thread.git_commits.push(sha.clone());
		}
	}

	// Dirty state
	if thread.git_start_dirty.is_none() {
		thread.git_start_dirty = status.is_dirty;
	}
	thread.git_end_dirty = status.is_dirty;
}
```

### 6.2 Integration Points

- **`create_new_thread()`**: Call `snapshot_git_state()` after creating thread
- **Before each sync**: Call `snapshot_git_state()` to capture any new commits
- **Message send / tool execution**: Optionally snapshot to track mid-session commits

---

## 7. Server-Side Processing

### 7.1 Repo ID Resolution

On thread upsert, resolve or create `repo_id`:

```rust
async fn get_or_create_repo_id(pool: &SqlitePool, slug: &str) -> Result<i64, ServerError> {
	let now = chrono::Utc::now().to_rfc3339();

	sqlx::query("INSERT OR IGNORE INTO repos (slug, created_at) VALUES(?, ?)")
		.bind(slug)
		.bind(&now)
		.execute(pool)
		.await?;

	let (id,): (i64,) = sqlx::query_as("SELECT id FROM repos WHERE slug = ?")
		.bind(slug)
		.fetch_one(pool)
		.await?;

	Ok(id)
}
```

### 7.2 Populate thread_commits

After upserting thread, insert commit records:

```rust
async fn record_commits(
	pool: &SqlitePool,
	thread: &Thread,
	repo_id: i64,
) -> Result<(), ServerError> {
	for sha in &thread.git_commits {
		let is_initial = Some(sha) == thread.git_initial_commit_sha.as_ref();
		let is_final = Some(sha) == thread.git_current_commit_sha.as_ref();

		sqlx::query(
			r#"
            INSERT OR IGNORE INTO thread_commits (
                thread_id, repo_id, commit_sha, branch, is_dirty,
                observed_at, is_initial, is_final
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
		)
		.bind(thread.id.as_str())
		.bind(repo_id)
		.bind(sha)
		.bind(&thread.git_branch)
		.bind(thread.git_end_dirty.unwrap_or(false) as i32)
		.bind(&thread.updated_at)
		.bind(is_initial as i32)
		.bind(is_final as i32)
		.execute(pool)
		.await?;
	}
	Ok(())
}
```

---

## 8. Testing Strategy

### Property-Based Tests

```rust
proptest! {
		/// **Property: Thread JSON roundtrip preserves all git metadata**
		///
		/// Why: Git metadata is persisted to disk and synced to server.
		/// Any data loss would break commit-based queries and time-travel.
		#[test]
		fn thread_git_metadata_roundtrip(
				branch in proptest::option::of("[a-z][a-z0-9/-]{0,50}"),
				remote in proptest::option::of("[a-z]+\\.[a-z]+/[a-z]+/[a-z]+"),
				commits in proptest::collection::vec("[0-9a-f]{40}", 0..10),
		) {
				let mut thread = Thread::new();
				thread.git_branch = branch.clone();
				thread.git_remote_url = remote.clone();
				thread.git_initial_branch = branch.clone();
				thread.git_initial_commit_sha = commits.first().cloned();
				thread.git_current_commit_sha = commits.last().cloned();
				thread.git_start_dirty = Some(false);
				thread.git_end_dirty = Some(true);
				thread.git_commits = commits.clone();

				let json = serde_json::to_string(&thread).unwrap();
				let restored: Thread = serde_json::from_str(&json).unwrap();

				prop_assert_eq!(restored.git_branch, branch);
				prop_assert_eq!(restored.git_remote_url, remote);
				prop_assert_eq!(restored.git_commits, commits);
		}

		/// **Property: Commit list only grows, never shrinks**
		///
		/// Why: Commits represent historical observations. Removing commits
		/// would break time-travel queries.
		#[test]
		fn commits_only_grow(
				initial_commits in proptest::collection::vec("[0-9a-f]{40}", 0..5),
				new_commits in proptest::collection::vec("[0-9a-f]{40}", 0..5),
		) {
				let mut thread = Thread::new();
				thread.git_commits = initial_commits.clone();

				// Simulate adding new commits (deduped)
				for sha in &new_commits {
						if !thread.git_commits.contains(sha) {
								thread.git_commits.push(sha.clone());
						}
				}

				// All initial commits still present
				for sha in &initial_commits {
						prop_assert!(thread.git_commits.contains(sha));
				}
		}
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_thread_commits_populated() {
	let (repo, _dir) = create_test_repo().await;

	let mut thread = create_test_thread();
	thread.git_remote_url = Some("github.com/test/repo".to_string());
	thread.git_commits = vec!["abc123".to_string(), "def456".to_string()];
	thread.git_initial_commit_sha = Some("abc123".to_string());
	thread.git_current_commit_sha = Some("def456".to_string());

	repo.upsert(&thread, None).await.unwrap();

	// Verify thread_commits table populated
	let commits = repo.get_thread_commits(&thread.id).await.unwrap();
	assert_eq!(commits.len(), 2);
}

#[tokio::test]
async fn test_find_threads_by_commit() {
	let (repo, _dir) = create_test_repo().await;

	// Create thread with specific commits
	let mut thread = create_test_thread();
	thread.git_remote_url = Some("github.com/test/repo".to_string());
	thread.git_commits = vec!["abc123".to_string()];
	repo.upsert(&thread, None).await.unwrap();

	// Query by commit
	let threads = repo.find_by_commit("abc123").await.unwrap();
	assert_eq!(threads.len(), 1);
	assert_eq!(threads[0].id, thread.id);
}
```

---

## 9. Migration Strategy

### Backward Compatibility

- All new fields are optional with `skip_serializing_if`
- Existing threads have `NULL` for new columns
- Old clients ignore unknown JSON fields
- `git_branch` and `git_remote_url` remain for simple queries

### Rollout Plan

1. **Phase 1**: Update loom-git with `detect_repo_status()`, `head_commit_sha()`, `is_dirty()`
2. **Phase 2**: Update loom-thread with new fields
3. **Phase 3**: Deploy migration 004 (repos table, thread extensions, thread_commits)
4. **Phase 4**: Update loom-server to populate repos and thread_commits
5. **Phase 5**: Update loom-cli to snapshot git state on create and update

---

## 10. Future Considerations

### 10.1 PR/MR Tracking

Add tables for pull requests and their commit associations:

```sql
CREATE TABLE pull_requests (
    id INTEGER PRIMARY KEY,
    repo_id INTEGER REFERENCES repos(id),
    pr_number INTEGER NOT NULL,
    title TEXT,
    url TEXT,
    UNIQUE (repo_id, pr_number)
);

CREATE TABLE thread_pull_requests (
    thread_id TEXT REFERENCES threads(id),
    pr_id INTEGER REFERENCES pull_requests(id),
    PRIMARY KEY (thread_id, pr_id)
);
```

### 10.2 Branch Rename Tracking

Track branch name changes during a session for better analytics.

### 10.3 Commit Graph Analytics

For advanced range queries, consider:

- Storing parent commit relationships
- Pre-computing merge-base information
- Integration with external git hosting APIs

---

## Appendix A: Cargo.toml for loom-git

```toml
[package]
name = "loom-git"
version.workspace = true
edition.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "Git repository detection for Loom"

[dependencies]
thiserror.workspace = true
tracing.workspace = true

[dev-dependencies]
tempfile.workspace = true
proptest.workspace = true
```

## Appendix B: Example API Usage

### Query threads by commit

```bash
# Find threads that touched a specific commit
curl "http://localhost:8080/v1/threads?commit=abc123def456"

# Response
{
  "threads": [
    {
      "id": "T-019b2b97-...",
      "title": "Implement feature X",
      "git_branch": "feature/x",
      "git_remote_url": "github.com/alice/my-app",
      "git_initial_commit_sha": "abc123def456",
      "git_current_commit_sha": "xyz789012345",
      ...
    }
  ],
  "total": 1
}
```

### Query threads by repository

```bash
curl "http://localhost:8080/v1/threads?git_remote_url=github.com/alice/my-app"
```

### Query threads with dirty state

```bash
curl "http://localhost:8080/v1/threads?git_start_dirty=true"
```
