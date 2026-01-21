# Local-First Session Persistence

**Status:** Draft
**Version:** 1.0
**Last Updated:** 2026-01-21

---

## 1. Overview
### Purpose
Persist session and thread data locally so Maestro can resume work across restarts. Provide an
optional sync queue for future backup or multi-device access.

### Goals
- Crash-safe, atomic local persistence.
- Stable schema for threads, sessions, messages, tool runs.
- Privacy controls for local-only threads.
- Optional sync queue with retry/backoff.

### Non-Goals
- Full account system or cloud auth.
- Real-time multi-device conflict resolution.
- UI design for sync settings.

---

## 2. Architecture
### Components
- **ThreadStore**: CRUD for thread metadata and snapshots.
- **SessionStore**: CRUD for session runtime records.
- **MessageStore**: append-only message log.
- **SyncQueue** (optional): durable queue of upsert/delete intents.
- **IndexStore**: small index for recent threads and fast list queries.

### Dependencies
- Tauri `app_data_dir()` for storage root.
- `serde` for JSON serialization.
- `specs/agent-state-machine.md` for state snapshots.

### Module/Folder Layout
- `app/src-tauri/src/storage/mod.rs` (new)
- `app/src-tauri/src/storage/thread_store.rs` (new)
- `app/src-tauri/src/storage/session_store.rs` (new)
- `app/src-tauri/src/storage/message_store.rs` (new)
- `app/src-tauri/src/storage/sync_queue.rs` (new, optional)
- `app/src/types/session.ts` (new shared types)

---

## 3. Data Model
### Core Types
| Type | Purpose | Notes |
| --- | --- | --- |
| `ThreadRecord` | User-visible conversation container | One per workspace context |
| `SessionRecord` | Runtime instance of an agent | Linked to a thread |
| `MessageRecord` | User/assistant/tool message | Append-only log |
| `ToolRunRecord` | Tool execution summary | Mirrors state machine |
| `SyncQueueItem` | Pending sync intent | Optional feature |

### ThreadRecord (JSON)
```json
{
  "schemaVersion": 1,
  "id": "thr_123",
  "title": "Add logging",
  "createdAt": "2026-01-21T10:00:00Z",
  "updatedAt": "2026-01-21T10:12:00Z",
  "projectPath": "/Users/laurence/dev/maestro",
  "harness": "opencode",
  "model": "gpt-4.1",
  "lastSessionId": "ses_456",
  "stateSnapshot": {
    "state": "Ready",
    "pendingToolCalls": []
  },
  "privacy": {
    "localOnly": true,
    "redactInputs": false,
    "redactOutputs": false
  },
  "metadata": {
    "tags": ["logging"],
    "pinned": false
  }
}
```

### SessionRecord (JSON)
```json
{
  "schemaVersion": 1,
  "id": "ses_456",
  "threadId": "thr_123",
  "status": "running",
  "startedAt": "2026-01-21T10:02:00Z",
  "endedAt": null,
  "workspaceRoot": "/Users/laurence/dev/maestro",
  "agent": {
    "harness": "opencode",
    "configHash": "sha256:...",
    "env": {"TERM": "xterm-256color"}
  },
  "toolRuns": [
    { "runId": "tool_1", "toolName": "edit_file", "status": "Succeeded" }
  ]
}
```

### MessageRecord (JSON)
```json
{
  "schemaVersion": 1,
  "id": "msg_001",
  "threadId": "thr_123",
  "sessionId": "ses_456",
  "role": "assistant",
  "content": "Added the logging call...",
  "createdAt": "2026-01-21T10:04:00Z",
  "toolCallId": null
}
```

### SyncQueueItem (JSON)
```json
{
  "schemaVersion": 1,
  "id": "sq_789",
  "entity": "thread",
  "entityId": "thr_123",
  "op": "upsert",
  "payloadHash": "sha256:...",
  "attempts": 0,
  "nextAttemptAt": "2026-01-21T10:15:00Z",
  "createdAt": "2026-01-21T10:13:00Z"
}
```

### Storage Schema
All paths are rooted at `app_data_dir()/sessions`.
- `threads/<thread_id>.json`
- `sessions/<session_id>.json`
- `messages/<thread_id>/<message_id>.json`
- `index.json`
- `sync_queue/<item_id>.json`
- `corrupt/<filename>.json`

---

## 4. Interfaces
### Public APIs
Tauri commands exposed to the frontend:
- `list_threads() -> ThreadSummary[]`
- `load_thread(thread_id) -> ThreadRecord`
- `save_thread(thread: ThreadRecord) -> ThreadRecord`
- `create_session(thread_id, harness, config) -> SessionRecord`
- `mark_session_ended(session_id, status) -> void`
- `append_message(message: MessageRecord) -> void`

### Internal APIs
- `write_atomic(path, bytes) -> Result<()>`
- `read_json<T>(path) -> Result<T>`
- `enqueue_sync(item: SyncQueueItem) -> Result<()>`

### Events (names + payloads)
- `session:persisted` `{ threadId, sessionId, updatedAt }`
- `session:resumed` `{ threadId, sessionId }`
- `session:persistence_failed` `{ threadId?, sessionId?, code, message }`
- `sync:enqueued` `{ entity, entityId, op }`
- `sync:failed` `{ itemId, code, attempts }`

---

## 5. Workflows
### Main Flow (Save)
```
UI -> save_thread -> ThreadStore.save
  -> serialize -> write_atomic(thread.json)
  -> update index -> write_atomic(index.json)
  -> enqueue sync item (if enabled and !localOnly)
```

### Resume Flow
```
UI -> load_thread -> SessionStore.load(lastSessionId)
  -> if session missing or ended: create_session
  -> emit session:resumed
```

### Edge Cases
- Missing index: rebuild by scanning `threads/`.
- Corrupt JSON: move to `corrupt/` and return error.
- Disk full: emit `session:persistence_failed` and keep in-memory state.

### Retry/Backoff
- Sync queue retries with exponential backoff (min 10s, max 5m, jitter).
- Stop retry after 10 attempts; mark item failed.

---

## 6. Error Handling
### Error Types
- `storage_unavailable`
- `serialization_failed`
- `atomic_write_failed`
- `schema_version_mismatch`
- `sync_transport_failed`

### Recovery Strategy
- Fallback to most recent backup if present.
- On schema mismatch, return a clear error with required version.

---

## 7. Observability
### Logs
- `storage.write.success`, `storage.write.failure`
- `storage.read.failure`
- `sync.enqueue`, `sync.attempt`, `sync.fail`

### Metrics
- `storage_write_ms`
- `storage_read_ms`
- `sync_queue_depth`
- `sync_attempts_total`

### Traces
- Session lifecycle span includes save/resume operations.

---

## 8. Security and Privacy
### AuthZ/AuthN
- Local storage only unless sync is enabled.

### Data Handling
- `privacy.localOnly` blocks sync queue creation.
- Optional redaction on input/output before persistence if configured.

---

## 9. Migration or Rollout
### Compatibility Notes
- Every record includes `schemaVersion` for future migrations.

### Rollout Plan
1. Implement local-only persistence.
2. Add index + resume support.
3. Add optional sync queue behind feature flag.

---

## 10. Open Questions
- Do we store messages as separate files or inline within threads?
- Default retention/cleanup policy for old sessions?
- Should tool outputs be stored verbatim or summarized?
