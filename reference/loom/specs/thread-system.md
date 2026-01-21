<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Thread Persistence System Specification

**Status:** Draft\
**Version:** 1.0\
**Last Updated:** 2024-12-17

---

## 1. Overview

### Purpose

The Thread Persistence System enables Loom to save, sync, and restore conversation sessions
(threads) across CLI invocations. A thread is a JSON document representing a complete conversation
with the LLM, including messages, agent state, and metadata.

### Goals

- **Persistence**: Save conversation state locally so sessions can be resumed
- **Synchronization**: Sync threads to a central server for backup, multi-device access, and
  analytics
- **Offline-First**: Always write locally first; sync is best-effort and non-blocking
- **XDG Compliance**: Follow XDG Base Directory Specification for local storage
- **Type Safety**: Shared Rust types between client and server

### Non-Goals

- Real-time collaboration on threads
- End-to-end encryption (out of scope for v1)
- Full-text search across messages (future enhancement)
- Conflict resolution UI (manual sync commands can be added later)

---

## 2. Thread Data Model

### Thread Identifier

Thread IDs use UUID7 (time-sortable) with a `T-` prefix:

```
T-019b2b97-fddf-7602-a3e4-1c4a295110c0
```

Properties:

- **Time-sorted**: UUID7 embeds timestamp, enabling chronological ordering
- **Globally unique**: No coordination required between clients
- **Human-readable prefix**: `T-` distinguishes threads from other IDs

### Thread JSON Schema

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

	"provider": "anthropic",
	"model": "claude-sonnet-4-20250514",

	"conversation": {
		"messages": [
			{
				"id": "m-019b2b97-fddf-7602-a3e4-000000000001",
				"role": "user",
				"content": "How do I add logging?",
				"created_at": "2025-01-01T12:00:01Z"
			},
			{
				"id": "m-019b2b97-fddf-7602-a3e4-000000000002",
				"role": "assistant",
				"content": "You can use the tracing crate...",
				"tool_calls": [],
				"created_at": "2025-01-01T12:00:05Z"
			}
		]
	},

	"agent_state": {
		"kind": "waiting_for_user_input",
		"retries": 0,
		"last_error": null,
		"pending_tool_calls": []
	},

	"visibility": "organization",
	"is_private": false,
	"is_shared_with_support": false,

	"metadata": {
		"title": "Add logging to my app",
		"tags": ["logging", "tracing"],
		"is_pinned": false,
		"extra": {}
	}
}
```

### Rust Type Definitions

```rust
/// Unique identifier for a thread
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub String);

impl ThreadId {
	/// Create a new thread ID with UUID7
	pub fn new() -> Self {
		let uuid = uuid7::uuid7();
		Self(format!("T-{}", uuid))
	}

	/// Parse an existing thread ID string
	pub fn parse(s: &str) -> Result<Self, ThreadIdError> {
		if !s.starts_with("T-") {
			return Err(ThreadIdError::InvalidPrefix);
		}
		// Validate UUID7 portion
		let uuid_part = &s[2..];
		uuid7::Uuid::parse_str(uuid_part).map_err(|_| ThreadIdError::InvalidUuid)?;
		Ok(Self(s.to_string()))
	}
}

/// Thread visibility controls how synced threads are exposed on the server.
/// - Organization: visible to organization members (default)
/// - Private: synced but only owner can see
/// - Public: may be listed/exposed publicly
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreadVisibility {
	Organization,
	Private,
	Public,
}

impl Default for ThreadVisibility {
	fn default() -> Self {
		ThreadVisibility::Organization
	}
}

/// Complete thread document
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Thread {
	pub id: ThreadId,
	pub version: u64,
	pub created_at: String,       // RFC3339
	pub updated_at: String,       // RFC3339
	pub last_activity_at: String, // RFC3339

	pub workspace_root: Option<String>,
	pub cwd: Option<String>,
	pub loom_version: Option<String>,

	pub provider: Option<String>,
	pub model: Option<String>,

	pub visibility: ThreadVisibility,
	pub is_private: bool, // If true, thread is local-only and NEVER syncs
	pub is_shared_with_support: bool, // If true, thread has been shared with support team

	pub conversation: ConversationSnapshot,
	pub agent_state: AgentStateSnapshot,
	pub metadata: ThreadMetadata,
}

/// Snapshot of the conversation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationSnapshot {
	pub messages: Vec<MessageSnapshot>,
}

/// Individual message in a conversation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageSnapshot {
	pub role: MessageRole,
	pub content: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_call_id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub tool_calls: Option<Vec<ToolCallSnapshot>>,
}

/// Snapshot of an individual tool call
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallSnapshot {
	pub id: String,
	pub tool_name: String,
	pub arguments_json: serde_json::Value,
}

/// Snapshot of agent state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentStateSnapshot {
	pub kind: AgentStateKind,
	pub retries: u32,
	pub last_error: Option<String>,
	pub pending_tool_calls: Vec<String>,
}

/// Enumeration of agent states (mirrors loom-core::AgentState variants)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStateKind {
	WaitingForUserInput,
	CallingLlm,
	ProcessingLlmResponse,
	ExecutingTools,
	PostToolsHook,
	Error,
	ShuttingDown,
}

/// Thread metadata
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ThreadMetadata {
	pub title: Option<String>,
	pub tags: Vec<String>,
	pub is_pinned: bool,
	pub extra: serde_json::Value,
}

/// Summary for list endpoints
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadSummary {
	pub id: ThreadId,
	pub title: Option<String>,
	pub workspace_root: Option<String>,
	pub last_activity_at: String,
	pub provider: Option<String>,
	pub model: Option<String>,
	pub tags: Vec<String>,
	pub version: u64,
	pub message_count: u32,
}
```

---

## 3. Sync Triggers

Threads are persisted at two key points:

### 3.1 After Inferencing Turn Completes

**Definition**: An inferencing turn completes when the agent returns to `WaitingForUserInput` after
processing user input, LLM response, and any tool executions.

**State Machine Integration**:

```
UserInput → CallingLlm → ProcessingLlmResponse → ExecutingTools → CallingLlm → ... → WaitingForUserInput
                                                                                              ↑
                                                                                       SYNC HERE
```

In the REPL driver (`loom-cli`), after `handle_event()` returns `AgentAction::WaitForInput` and
state is `WaitingForUserInput`:

```rust
loop {
    let action = agent.handle_event(event)?;
    match action {
        AgentAction::WaitForInput => {
            if matches!(agent.state(), AgentState::WaitingForUserInput { .. }) {
                // Inferencing turn complete - sync thread
                thread.update_from_agent(&agent);
                thread.version += 1;
                thread.updated_at = now_rfc3339();
                thread.last_activity_at = now_rfc3339();
                thread_store.save(&thread).await?;
            }
        }
        // ... other actions
    }
}
```

### 3.2 On Graceful Shutdown

**Definition**: When Loom CLI exits via:

- `ShutdownRequested` event (SIGINT, Ctrl+C)
- End of input (EOF)
- Explicit `/exit` command

```rust
AgentAction::Shutdown => {
    thread.update_from_agent(&agent);
    thread.version += 1;
    thread.updated_at = now_rfc3339();
    thread_store.save(&thread).await?;
    break;
}
```

---

## 4. CLI Thread Commands

The Loom CLI provides commands for managing and resuming threads using the `ThreadStore`
abstraction.

### 4.1 Commands

- `loom` - Starts new interactive REPL session, creates new Thread
- `loom list` - Lists local threads using ThreadStore::list, sorted by last_activity_at descending
- `loom resume` - Resumes most recent thread
- `loom resume <thread_id>` - Resumes specific thread by ID
- `loom search <query>` - Searches threads by content, git metadata, or commit SHA

### 4.2 Version and Update Commands

- `loom version` - Shows version information including:
  - Package version from Cargo.toml
  - Git SHA (short commit hash)
  - Build timestamp (RFC3339)
  - Build age (relative and absolute)
  - Platform (os-arch, e.g., `linux-x86_64`)

- `loom update` - Self-updates the CLI binary:
  - Downloads the latest binary from the server at `/bin/{platform}`
  - Replaces the current executable atomically
  - Creates a backup of the old binary (`.old` extension)
  - Requires `LOOM_UPDATE_BASE_URL` or `LOOM_THREAD_SYNC_URL` to be set

### 4.3 Version Headers

All HTTP requests from the CLI to the loom-server include version headers:

- `X-Loom-Version` - Package version (e.g., `0.1.0`)
- `X-Loom-Git-Sha` - Git commit SHA
- `X-Loom-Build-Timestamp` - Build time (RFC3339)
- `X-Loom-Platform` - Target platform (e.g., `linux-x86_64`)

### 4.4 Auth Commands (Stubs)

- `loom login` - Stub, not implemented yet
- `loom logout` - Stub, not implemented yet

Server stub endpoints:

- POST /v1/auth/login - returns 501 Not Implemented
- POST /v1/auth/logout - returns 501 Not Implemented

### 4.5 Binary Distribution

The server serves pre-built CLI binaries at `/bin/{platform}`:

- `GET /bin/linux-x86_64` - Linux x86_64 binary
- `GET /bin/linux-aarch64` - Linux ARM64 binary
- `GET /bin/macos-x86_64` - macOS Intel binary
- `GET /bin/macos-aarch64` - macOS Apple Silicon binary
- `GET /bin/windows-x86_64` - Windows x64 binary

Platform string format: `{CARGO_CFG_TARGET_OS}-{CARGO_CFG_TARGET_ARCH}`

The server reads binaries from `$LOOM_SERVER_BIN_DIR` (default: `./bin`).

### 4.6 Example Usage

```bash
# Start new session
loom

# List threads
loom list

# Resume most recent
loom resume

# Resume specific thread
loom resume T-019b2b97-fddf-7602-a3e4-1c4a295110c0

# Search threads
loom search "authentication fix"
loom search --limit 10 "refactor"

# Show version info
loom version

# Update to latest version
loom update

# Login/logout stubs
loom login
loom logout
```

### 4.7 Private and Share Commands

- `loom private` - Starts a new private (local-only) session that NEVER syncs to the server. Sets
  `is_private = true` on the thread.

- `loom share [threadId] --visibility [organization|private|public]` - Changes the server-side
  visibility of a synced thread.
  - Cannot be used on private (local-only) threads
  - If no threadId provided, uses most recent thread

- `loom share [threadId] --support` - Shares the thread with the support team by setting
  `is_shared_with_support = true`.
  - Does NOT change the thread's visibility setting
  - Cannot be used on private (local-only) threads

Example usage:

```bash
# Start a private session that never syncs
loom private

# Make the most recent thread publicly listed
loom share --visibility public

# Share a specific thread with support (visibility unchanged)
loom share T-019b2b97-fddf-7602-a3e4-1c4a295110c0 --support

# Change visibility to private
loom share T-019b2b97-fddf-7602-a3e4-1c4a295110c0 --visibility private
```

### 4.8 Sync Privacy Enforcement

**Invariant**: If `thread.is_private == true`, the thread MUST NEVER be sent to the server.

This is enforced at the `SyncingThreadStore` layer:

- `save()` checks `is_private` and skips server sync if true
- `delete()` checks `is_private` and skips server delete notification if true
- No HTTP requests are made for private threads

This ensures that even if sync is configured, private sessions remain completely local.

---

## 5. Local Storage

### 5.1 File Locations

Following XDG Base Directory Specification:

| Purpose            | Path                                           |
| ------------------ | ---------------------------------------------- |
| Thread files       | `$XDG_DATA_HOME/loom/threads/<thread_id>.json` |
| Pending sync queue | `$XDG_STATE_HOME/loom/sync/pending.json`       |

### 5.2 LocalThreadStore

```rust
pub struct LocalThreadStore {
	threads_dir: PathBuf,
}

impl LocalThreadStore {
	pub fn new(threads_dir: PathBuf) -> Self {
		Self { threads_dir }
	}

	pub fn from_xdg() -> Result<Self, ThreadStoreError> {
		let data_dir = dirs::data_dir().ok_or_else(|| {
			ThreadStoreError::Io(std::io::Error::new(
				std::io::ErrorKind::NotFound,
				"could not determine XDG data directory",
			))
		})?;
		let threads_dir = data_dir.join("loom").join("threads");
		std::fs::create_dir_all(&threads_dir)?;
		Ok(Self::new(threads_dir))
	}

	fn thread_path(&self, id: &ThreadId) -> PathBuf {
		self.threads_dir.join(format!("{}.json", id))
	}

	/// Search threads locally using substring matching.
	/// Used as fallback when server is unavailable.
	pub async fn search(
		&self,
		query: &str,
		limit: usize,
	) -> Result<Vec<ThreadSummary>, ThreadStoreError>;
}

#[async_trait]
impl ThreadStore for LocalThreadStore {
	async fn load(&self, id: &ThreadId) -> Result<Option<Thread>, ThreadStoreError> {
		let path = self.thread_path(id);
		if !path.exists() {
			return Ok(None);
		}
		let contents = tokio::fs::read_to_string(&path).await?;
		let thread: Thread = serde_json::from_str(&contents)?;
		Ok(Some(thread))
	}

	async fn save(&self, thread: &Thread) -> Result<(), ThreadStoreError> {
		let path = self.thread_path(&thread.id);
		let contents = serde_json::to_string_pretty(thread)?;
		// Atomic write: write to temp file, then rename
		let temp_path = path.with_extension("json.tmp");
		tokio::fs::write(&temp_path, &contents).await?;
		tokio::fs::rename(&temp_path, &path).await?;
		tracing::debug!(thread_id = %thread.id.0, version = thread.version, "thread saved locally");
		Ok(())
	}

	async fn list(&self, limit: u32) -> Result<Vec<ThreadSummary>, ThreadStoreError> {
		// Read all thread files, parse, sort by last_activity_at desc, take limit
	}

	async fn delete(&self, id: &ThreadId) -> Result<(), ThreadStoreError> {
		let path = self.thread_path(id);
		if path.exists() {
			tokio::fs::remove_file(&path).await?;
		}
		Ok(())
	}
}
```

### 5.3 Pending Sync Queue

When background sync fails (e.g., network unavailable), failed operations are persisted to
`$XDG_STATE_HOME/loom/sync/pending.json` for later retry.

#### Data Model

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingSyncEntry {
	pub thread_id: ThreadId,
	pub operation: SyncOperation,
	pub failed_at: String, // RFC3339
	pub retry_count: u32,
	pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncOperation {
	Upsert,
	Delete,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PendingSyncQueue {
	pub entries: Vec<PendingSyncEntry>,
}
```

#### PendingSyncStore

```rust
pub struct PendingSyncStore {
	path: PathBuf, // $XDG_STATE_HOME/loom/sync/pending.json
}

impl PendingSyncStore {
	pub fn from_xdg() -> Result<Self, ThreadStoreError>;
	pub async fn load(&self) -> Result<PendingSyncQueue, ThreadStoreError>;
	pub async fn save(&self, queue: &PendingSyncQueue) -> Result<(), ThreadStoreError>;
	pub async fn add_pending(
		&self,
		thread_id: ThreadId,
		operation: SyncOperation,
		error: Option<String>,
	) -> Result<(), ThreadStoreError>;
	pub async fn remove_pending(
		&self,
		thread_id: &ThreadId,
		operation: &SyncOperation,
	) -> Result<(), ThreadStoreError>;
	pub async fn clear(&self) -> Result<(), ThreadStoreError>;
}
```

#### Behavior

1. **On sync failure**: `SyncingThreadStore.save()` spawns a background task that, on failure, adds
   an entry to the pending queue via `PendingSyncStore.add_pending()`.

2. **On sync success**: The entry is removed from the pending queue (if it existed).

3. **Retry mechanism**: `SyncingThreadStore.retry_pending()` iterates through pending entries and
   attempts to sync each one. Successful syncs are removed from the queue.

4. **Deduplication**: If the same thread ID and operation already exists in the queue, the
   `retry_count` is incremented and `failed_at` is updated rather than adding a duplicate.

---

## 6. Server API Design

### 6.1 Endpoints

Base URL: `https://api.loom.example.com/v1`

| Method   | Endpoint        | Description             |
| -------- | --------------- | ----------------------- |
| `PUT`    | `/threads/{id}` | Create or update thread |
| `GET`    | `/threads/{id}` | Get thread by ID        |
| `GET`    | `/threads`      | List threads            |
| `DELETE` | `/threads/{id}` | Soft-delete thread      |

### 6.2 PUT /threads/{id}

**Request**:

```http
PUT /v1/threads/T-019b2b97-fddf-7602-a3e4-1c4a295110c0
Content-Type: application/json
If-Match: 4

{
  "id": "T-019b2b97-fddf-7602-a3e4-1c4a295110c0",
  "version": 5,
  ...
}
```

**Responses**:

| Status            | Description      | Body                                                              |
| ----------------- | ---------------- | ----------------------------------------------------------------- |
| `200 OK`          | Thread upserted  | `Thread` (server's version)                                       |
| `409 Conflict`    | Version mismatch | `{"error": "conflict", "server_version": 6, "client_version": 5}` |
| `400 Bad Request` | Invalid payload  | `{"error": "invalid_request", "message": "..."}`                  |

**Behavior**:

1. If thread doesn't exist → Insert with `version` from payload
2. If exists and `If-Match` header matches → Update
3. If exists and `If-Match` doesn't match → 409 Conflict

### 6.3 GET /threads/{id}

**Request**:

```http
GET /v1/threads/T-019b2b97-fddf-7602-a3e4-1c4a295110c0
```

**Responses**:

| Status          | Description      | Body                     |
| --------------- | ---------------- | ------------------------ |
| `200 OK`        | Thread found     | `Thread` JSON            |
| `404 Not Found` | Thread not found | `{"error": "not_found"}` |

### 6.4 GET /threads

**Request**:

```http
GET /v1/threads?workspace=/home/alice/projects&limit=50
```

**Query Parameters**:

| Parameter   | Type   | Default | Description              |
| ----------- | ------ | ------- | ------------------------ |
| `workspace` | string | -       | Filter by workspace root |
| `limit`     | u32    | 50      | Max results              |
| `offset`    | u32    | 0       | Pagination offset        |

**Response**:

```json
{
	"threads": [
		{
			"id": "T-019b2b97-...",
			"title": "Add logging",
			"workspace_root": "/home/alice/projects",
			"last_activity_at": "2025-01-01T12:05:00Z",
			"provider": "anthropic",
			"model": "claude-sonnet-4-20250514",
			"tags": ["logging"],
			"version": 5,
			"message_count": 10
		}
	],
	"total": 100,
	"limit": 50,
	"offset": 0
}
```

### 6.5 DELETE /threads/{id}

**Request**:

```http
DELETE /v1/threads/T-019b2b97-fddf-7602-a3e4-1c4a295110c0
```

**Responses**:

| Status           | Description      |
| ---------------- | ---------------- |
| `204 No Content` | Thread deleted   |
| `404 Not Found`  | Thread not found |

**Behavior**: Soft-delete by setting `deleted_at` timestamp.

---

## 7. SQLite Schema

### 7.1 Database Configuration

```rust
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous};

pub async fn create_pool(database_url: &str) -> Result<SqlitePool, DbError> {
	let options = SqliteConnectOptions::from_str(database_url)?
		.journal_mode(SqliteJournalMode::Wal)
		.synchronous(SqliteSynchronous::Normal)
		.create_if_missing(true);

	let pool = SqlitePool::connect_with(options).await?;

	// Run migrations
	sqlx::migrate!("./migrations").run(&pool).await?;

	Ok(pool)
}
```

**WAL Mode Benefits**:

- Multiple concurrent readers
- Single writer (with row-level locking)
- Better crash recovery
- Improved performance for read-heavy workloads

### 7.2 Table Schema

```sql
-- migrations/001_create_threads.sql

CREATE TABLE IF NOT EXISTS threads (
    id TEXT PRIMARY KEY,                 -- "T-019b2b97-..."
    version INTEGER NOT NULL,            -- Optimistic concurrency
    
    created_at TEXT NOT NULL,            -- RFC3339
    updated_at TEXT NOT NULL,            -- RFC3339
    last_activity_at TEXT NOT NULL,      -- RFC3339
    deleted_at TEXT,                     -- Soft delete marker
    
    -- Denormalized for querying
    workspace_root TEXT,
    cwd TEXT,
    loom_version TEXT,
    provider TEXT,
    model TEXT,
    
    title TEXT,
    tags TEXT,                           -- JSON array
    is_pinned INTEGER NOT NULL DEFAULT 0,
    visibility TEXT NOT NULL DEFAULT 'private',
    
    message_count INTEGER NOT NULL DEFAULT 0,
    
    -- Full document storage
    agent_state_kind TEXT NOT NULL,
    agent_state JSON NOT NULL,
    conversation JSON NOT NULL,
    metadata JSON NOT NULL,
    full_json JSON NOT NULL              -- Complete Thread for evolution
);

-- Index for listing by workspace
CREATE INDEX IF NOT EXISTS idx_threads_workspace_activity
    ON threads (workspace_root, last_activity_at DESC)
    WHERE deleted_at IS NULL;

-- Index for soft-delete filtering
CREATE INDEX IF NOT EXISTS idx_threads_deleted
    ON threads (deleted_at);

-- Index for pinned threads
CREATE INDEX IF NOT EXISTS idx_threads_pinned
    ON threads (is_pinned, last_activity_at DESC)
    WHERE deleted_at IS NULL;
```

---

## 8. Client Sync Architecture

### 8.1 ThreadSyncClient

```rust
pub struct ThreadSyncClient {
	base_url: url::Url,
	http: reqwest::Client,
	retry_config: RetryConfig,
}

impl ThreadSyncClient {
	pub fn new(base_url: &str, retry_config: RetryConfig) -> Result<Self, ThreadSyncError> {
		Ok(Self {
			base_url: url::Url::parse(base_url)?,
			http: reqwest::Client::builder()
				.timeout(Duration::from_secs(30))
				.build()?,
			retry_config,
		})
	}

	pub async fn upsert_thread(&self, thread: &Thread) -> Result<Thread, ThreadSyncError> {
		let url = self.base_url.join(&format!("v1/threads/{}", thread.id.0))?;

		let response = retry(&self.retry_config, || {
			let req = self
				.http
				.put(url.clone())
				.header("Content-Type", "application/json")
				.header("If-Match", thread.version.to_string())
				.json(thread);
			async move { req.send().await }
		})
		.await?;

		match response.status() {
			StatusCode::OK => {
				let server_thread: Thread = response.json().await?;
				Ok(server_thread)
			}
			StatusCode::CONFLICT => {
				let conflict: ConflictResponse = response.json().await?;
				Err(ThreadSyncError::Conflict {
					local: thread.version,
					remote: conflict.server_version,
				})
			}
			status => Err(ThreadSyncError::UnexpectedStatus {
				status: status.as_u16(),
			}),
		}
	}

	pub async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>, ThreadSyncError> {
		// ...
	}

	pub async fn list_threads(&self, params: ListParams) -> Result<ListResponse, ThreadSyncError> {
		// ...
	}

	pub async fn delete_thread(&self, id: &ThreadId) -> Result<(), ThreadSyncError> {
		// ...
	}
}
```

### 8.2 SyncingThreadStore

Wraps `LocalThreadStore` and adds server sync with pending queue support:

```rust
pub struct SyncingThreadStore {
	local: LocalThreadStore,
	sync_client: Option<ThreadSyncClient>,
	pending_store: Option<Arc<Mutex<PendingSyncStore>>>,
}

impl SyncingThreadStore {
	pub fn new(local: LocalThreadStore, sync_client: Option<ThreadSyncClient>) -> Self;
	pub fn local_only(local: LocalThreadStore) -> Self;
	pub fn with_sync(local: LocalThreadStore, sync_client: ThreadSyncClient) -> Self;
	pub fn with_pending_store(self, pending_store: PendingSyncStore) -> Self;

	/// Retry all pending sync operations. Returns count of successful retries.
	pub async fn retry_pending(&self) -> Result<usize, ThreadStoreError>;

	/// Get the number of pending sync operations.
	pub async fn pending_count(&self) -> usize;
}

#[async_trait]
impl ThreadStore for SyncingThreadStore {
	async fn save(&self, thread: &Thread) -> Result<(), ThreadStoreError> {
		// Always save locally first
		self.local.save(thread).await?;

		// Skip sync for private threads
		if thread.is_private {
			return Ok(());
		}

		// Sync to server in background (fire-and-forget)
		if let Some(ref client) = self.sync_client {
			let thread_clone = thread.clone();
			let pending_store = self.pending_store.clone();

			tokio::spawn(async move {
				match client.upsert_thread(&thread_clone).await {
					Ok(_) => {
						// Remove from pending queue if it was there
						if let Some(store) = pending_store {
							let store = store.lock().await;
							let _ = store
								.remove_pending(&thread_clone.id, &SyncOperation::Upsert)
								.await;
						}
					}
					Err(e) => {
						tracing::warn!(
								thread_id = %thread_clone.id.0,
								error = %e,
								"thread sync failed (local save succeeded)"
						);
						// Add to pending queue for retry
						if let Some(store) = pending_store {
							let store = store.lock().await;
							let _ = store
								.add_pending(
									thread_clone.id.clone(),
									SyncOperation::Upsert,
									Some(e.to_string()),
								)
								.await;
						}
					}
				}
			});
		}

		Ok(())
	}

	/// Save locally and wait for sync to complete (blocking).
	async fn save_and_sync(&self, thread: &Thread) -> Result<(), ThreadStoreError>;

	// ... other methods delegate to local
}
```

---

## 9. Error Types

### 9.1 Thread Store Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum ThreadStoreError {
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),

	#[error("Serialization error: {0}")]
	Serialization(#[from] serde_json::Error),

	#[error("Thread not found: {0}")]
	NotFound(String),

	#[error("Sync error: {0}")]
	Sync(#[from] ThreadSyncError),
}
```

### 9.2 Sync Errors

```rust
#[derive(Debug, Clone, thiserror::Error)]
pub enum ThreadSyncError {
	#[error("Network error: {0}")]
	Network(String),

	#[error("Server error: {status} - {message}")]
	Server { status: u16, message: String },

	#[error("Conflict: local version {local}, server version {remote}")]
	Conflict { local: u64, remote: u64 },

	#[error("Invalid URL: {0}")]
	InvalidUrl(String),

	#[error("Timeout")]
	Timeout,

	#[error("Unexpected status: {status}")]
	UnexpectedStatus { status: u16 },
}

impl loom_http::RetryableError for ThreadSyncError {
	fn is_retryable(&self) -> bool {
		matches!(self,
				ThreadSyncError::Network(_)
				| ThreadSyncError::Timeout
				| ThreadSyncError::Server { status, .. } if *status >= 500
		)
	}
}
```

### 9.3 Server Errors

```rust
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
	#[error("Database error: {0}")]
	Db(#[from] sqlx::Error),

	#[error("Thread not found: {0}")]
	NotFound(String),

	#[error("Version conflict: expected {expected}, got {actual}")]
	Conflict { expected: u64, actual: u64 },

	#[error("Invalid request: {0}")]
	BadRequest(String),

	#[error("Internal error: {0}")]
	Internal(String),
}
```

---

## 10. Crate Structure

```
loom/
└── crates/
    ├── loom-core/           # Existing: core types, state machine
    ├── loom-thread/         # NEW: shared thread models, ThreadStore trait
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs       # Re-exports
    │       ├── model.rs     # Thread, ThreadId, snapshots
    │       ├── store.rs     # ThreadStore trait, LocalThreadStore
    │       ├── sync.rs      # ThreadSyncClient, SyncingThreadStore
    │       └── error.rs     # ThreadStoreError, ThreadSyncError
    │
    ├── loom-server/         # NEW: HTTP server with SQLite
    │   ├── Cargo.toml
    │   ├── migrations/      # SQLite migrations
    │   └── src/
    │       ├── main.rs      # Server binary entry point
    │       ├── lib.rs       # Library for testing
    │       ├── api.rs       # Axum routes and handlers
    │       ├── db.rs        # SQLite operations
    │       ├── error.rs     # ServerError
    │       └── config.rs    # Server configuration
    │
    └── loom-cli/            # Existing: integrate thread store
```

### Dependency Graph

```
                    ┌─────────────┐
                    │  loom-cli   │
                    └──────┬──────┘
                           │
        ┌──────────────────┼──────────────────┐
        │                  │                  │
        ▼                  ▼                  ▼
┌───────────────┐  ┌───────────────┐  ┌─────────────┐
│loom-llm-      │  │  loom-thread  │  │ loom-tools  │
│anthropic      │  │               │  │             │
└───────┬───────┘  └───────┬───────┘  └──────┬──────┘
        │                  │                 │
        │                  │                 │
        ▼                  ▼                 │
┌───────────────┐  ┌───────────────┐         │
│loom-http│  │  loom-core    │◄────────┘
└───────────────┘  └───────────────┘

                    ┌─────────────┐
                    │ loom-server │
                    └──────┬──────┘
                           │
                    ┌──────┴──────┐
                    ▼             ▼
            ┌─────────────┐ ┌─────────────┐
            │ loom-thread │ │   axum      │
            └─────────────┘ │   sqlx      │
                            └─────────────┘
```

---

## 11. Configuration

### 11.1 Client Configuration

In `config.toml`:

```toml
[thread_sync]
# Enable server sync (false = local only)
enabled = true

# Server URL
server_url = "https://threads.loom.example.com"

# Retry settings for sync
max_retries = 3
initial_backoff_ms = 200
max_backoff_ms = 5000
```

### 11.2 Server Configuration

Via environment variables:

| Variable                   | Default            | Description          |
| -------------------------- | ------------------ | -------------------- |
| `LOOM_SERVER_HOST`         | `127.0.0.1`        | Bind address         |
| `LOOM_SERVER_PORT`         | `8080`             | Bind port            |
| `LOOM_SERVER_DATABASE_URL` | `sqlite:./loom.db` | SQLite database path |
| `LOOM_SERVER_LOG_LEVEL`    | `info`             | Log level            |

---

## 12. Testing Strategy

### 12.1 Property-Based Tests

#### Thread Model

```rust
proptest! {
		/// **Property: Thread JSON roundtrip preserves all data**
		///
		/// Ensures serialization/deserialization is lossless for any valid thread.
		#[test]
		fn thread_json_roundtrip(thread in arb_thread()) {
				let json = serde_json::to_string(&thread).unwrap();
				let decoded: Thread = serde_json::from_str(&json).unwrap();
				prop_assert_eq!(thread, decoded);
		}

		/// **Property: ThreadId format is always valid**
		///
		/// All generated ThreadIds must start with "T-" and contain valid UUID7.
		#[test]
		fn thread_id_format(id in arb_thread_id()) {
				prop_assert!(id.0.starts_with("T-"));
				let uuid_part = &id.0[2..];
				prop_assert!(uuid7::Uuid::parse_str(uuid_part).is_ok());
		}

		/// **Property: Version is monotonically increasing**
		///
		/// After N mutations, version equals initial + N.
		#[test]
		fn version_monotonicity(
				initial_version in 0u64..1000,
				mutations in 1usize..100
		) {
				let mut thread = Thread::new();
				thread.version = initial_version;
				for _ in 0..mutations {
						thread.version += 1;
				}
				prop_assert_eq!(thread.version, initial_version + mutations as u64);
		}
}
```

#### Visibility

```rust
proptest! {
		/// **Property: Private threads never trigger sync**
		///
		/// Why this is important: Private sessions are a trust boundary. Users
		/// expect local-only threads to never leave their machine.
		///
		/// Invariant: SyncingThreadStore.save() never calls sync_client when is_private == true
		#[test]
		fn private_threads_never_sync(thread in arb_thread()) {
				let rt = tokio::runtime::Runtime::new().unwrap();
				rt.block_on(async {
						let mut private_thread = thread.clone();
						private_thread.is_private = true;

						let store = SyncingThreadStore::with_mock_sync(...);
						store.save(&private_thread).await.unwrap();

						prop_assert!(store.sync_calls() == 0);
						Ok(())
				}).unwrap();
		}

		/// **Property: ThreadVisibility serializes to lowercase**
		///
		/// Why this is important: API contracts expect lowercase visibility values.
		#[test]
		fn visibility_serde_format(_dummy in 0u8..1u8) {
				let variants = [
						(ThreadVisibility::Private, "\"private\""),
						(ThreadVisibility::Unlisted, "\"unlisted\""),
						(ThreadVisibility::Public, "\"public\""),
				];
				for (vis, expected) in variants {
						let json = serde_json::to_string(&vis).unwrap();
						prop_assert_eq!(json, expected);
				}
		}
}
```

#### Local Store

```rust
proptest! {
		/// **Property: LocalThreadStore save/load roundtrip**
		///
		/// Any thread saved to LocalThreadStore can be loaded back identically.
		#[test]
		fn local_store_roundtrip(thread in arb_thread()) {
				let rt = tokio::runtime::Runtime::new().unwrap();
				rt.block_on(async {
						let temp_dir = tempfile::tempdir().unwrap();
						let store = LocalThreadStore::new(temp_dir.path().to_path_buf());

						store.save(&thread).await.unwrap();
						let loaded = store.load(&thread.id).await.unwrap().unwrap();

						prop_assert_eq!(thread, loaded);
						Ok(())
				}).unwrap();
		}
}
```

#### Server API

```rust
proptest! {
		/// **Property: Upsert idempotency**
		///
		/// Upserting the same thread twice with same version succeeds.
		#[test]
		fn upsert_idempotent(thread in arb_thread()) {
				let rt = tokio::runtime::Runtime::new().unwrap();
				rt.block_on(async {
						let app = create_test_app().await;

						let response1 = upsert_thread(&app, &thread).await;
						prop_assert!(response1.status().is_success());

						let response2 = upsert_thread(&app, &thread).await;
						prop_assert!(response2.status().is_success());

						Ok(())
				}).unwrap();
		}

		/// **Property: Version conflict detection**
		///
		/// Upserting with stale version returns 409.
		#[test]
		fn version_conflict(thread in arb_thread()) {
				let rt = tokio::runtime::Runtime::new().unwrap();
				rt.block_on(async {
						let app = create_test_app().await;

						// Insert thread
						upsert_thread(&app, &thread).await;

						// Try to update with old version
						let stale_thread = Thread { version: thread.version.saturating_sub(1), ..thread };
						let response = upsert_thread(&app, &stale_thread).await;

						prop_assert_eq!(response.status(), StatusCode::CONFLICT);
						Ok(())
				}).unwrap();
		}
}
```

### 12.2 Integration Tests

- End-to-end test with CLI → LocalThreadStore → SyncingThreadStore → Server → SQLite
- Offline mode testing (server unavailable)
- Concurrent sync from multiple clients

---

## 13. Future Considerations

### 13.1 Authentication

- Add API key or JWT authentication to server endpoints
- Store credentials in system keyring
- Stub endpoints exist: POST /v1/auth/login and POST /v1/auth/logout (return 501 Not Implemented)

### 13.2 Search

- Add full-text search using SQLite FTS5:
  ```sql
  CREATE VIRTUAL TABLE threads_fts USING fts5(content, title, tags);
  ```

### 13.3 Real-Time Sync

- WebSocket connection for live updates
- Server-sent events for thread changes

### 13.4 Conflict Resolution

- Three-way merge for conversation histories
- CLI command: `loom thread resolve <id>`

### 13.5 Backup & Export

- `loom thread export <id> > thread.json`
- `loom thread import < thread.json`

---

## Appendix A: Example API Requests

### Create Thread

```bash
curl -X PUT "http://localhost:8080/v1/threads/T-019b2b97-fddf-7602-a3e4-1c4a295110c0" \
  -H "Content-Type: application/json" \
  -H "If-Match: 0" \
  -d '{
    "id": "T-019b2b97-fddf-7602-a3e4-1c4a295110c0",
    "version": 1,
    "created_at": "2025-01-01T12:00:00Z",
    "updated_at": "2025-01-01T12:00:00Z",
    "last_activity_at": "2025-01-01T12:00:00Z",
    "conversation": {"messages": []},
    "agent_state": {"kind": "waiting_for_user_input", "retries": 0, "pending_tool_calls": []},
    "metadata": {}
  }'
```

### Get Thread

```bash
curl "http://localhost:8080/v1/threads/T-019b2b97-fddf-7602-a3e4-1c4a295110c0"
```

### List Threads

```bash
curl "http://localhost:8080/v1/threads?limit=10&workspace=/home/alice/projects"
```

### Delete Thread

```bash
curl -X DELETE "http://localhost:8080/v1/threads/T-019b2b97-fddf-7602-a3e4-1c4a295110c0"
```
