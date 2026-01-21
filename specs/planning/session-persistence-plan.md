# Local-First Session Persistence Implementation Plan

Reference: [session-persistence.md](../session-persistence.md)

## Phase 1: Storage foundation
- [x] Add `storage/` module structure in `app/src-tauri/src` (See §2)
- [x] Implement `write_atomic` and `read_json` helpers (See §4, §5)
- [x] Define shared record types in `app/src/types/session.ts` (See §3)

Exit Criteria
- [x] Atomic write helper writes temp + rename without panic (See §5)
- [x] Record types compile in TS and Rust (See §3)
- [x] Storage root resolves under `app_data_dir()` (See §2)

## Phase 2: Thread + session CRUD
- [x] Implement `ThreadStore` and `SessionStore` (See §2, §4)
- [x] Add `list_threads`, `load_thread`, `save_thread` Tauri commands (See §4)
- [x] Add `create_session`, `mark_session_ended` commands (See §4)

Exit Criteria
- [x] Saving a thread creates `threads/<id>.json` (See §3, §5)
- [x] Listing threads returns persisted metadata (See §4)
- [x] Session record written on create and updated on end (See §4)

## Phase 3: Message storage
- [ ] Implement append-only `MessageStore` (See §3)
- [ ] Wire `append_message` command to session loop (See §4, §5)

Exit Criteria
- [ ] Message file created under `messages/<thread_id>/` (See §3)
- [ ] Appending preserves order when reloaded (See §5)

## Phase 4: Index and resume
- [ ] Build `IndexStore` and automatic rebuild when missing (See §5)
- [ ] Add resume flow and `session:resumed` event (See §5)

Exit Criteria
- [ ] Deleting `index.json` triggers rebuild on next list (See §5)
- [ ] Restarting app resumes last session for a thread (manual)

## Phase 5: Optional sync queue
- [ ] Implement `SyncQueue` with retry/backoff (See §2, §5)
- [ ] Guard sync behind feature flag and `privacy.localOnly` (See §8)

Exit Criteria
- [ ] Sync queue writes entries only when sync enabled (See §8)
- [ ] `localOnly` threads never enqueue sync items (See §8)

## Files to Create
- `app/src-tauri/src/storage/mod.rs`
- `app/src-tauri/src/storage/thread_store.rs`
- `app/src-tauri/src/storage/session_store.rs`
- `app/src-tauri/src/storage/message_store.rs`
- `app/src-tauri/src/storage/sync_queue.rs`
- `app/src/types/session.ts`

## Files to Modify
- `app/src-tauri/src/sessions.rs`
- `app/src-tauri/src/lib.rs`
- `app/src/services/sessions.ts`

## Verification Checklist
- [ ] `bun run typecheck`
- [ ] Manual: restart app and resume last thread
