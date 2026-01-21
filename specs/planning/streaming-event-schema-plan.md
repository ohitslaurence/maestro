# Unified Streaming Event Schema Implementation Plan

Reference: [streaming-event-schema.md](../streaming-event-schema.md)

## Phase 1: Core types and emit helpers
- [x] Add `StreamEvent` TS types in `app/src/types/streaming.ts` (See §3)
- [x] Add Rust `StreamEvent` struct + serializer in `app/src-tauri/src/sessions.rs` (See §3)
- [x] Add `emit_stream_event` helper in `app/src-tauri/src/lib.rs` (See §4)

Exit Criteria
- [x] `StreamEvent` types compile in both Rust and TS (See §3)
- [x] `emit_stream_event` emits `agent:stream_event` without panics (See §4)
- [x] Envelope includes `schemaVersion`, `eventId`, `streamId`, `seq` (See §3)

## Phase 2: Harness adapters
- [x] Map OpenCode stream output to `StreamEvent` in session broker (See §2, §5)
- [x] Map Claude Code stream output to `StreamEvent` in session broker (See §2, §5)
- [x] Attach `streamId` + `seq` + `eventId` for ordering (See §3, §5)

Exit Criteria
- [x] OpenCode streams emit ordered `text_delta` events (See §5)
- [x] Claude Code streams emit ordered `text_delta` events (See §5)
- [x] `completed` emitted once per `streamId` (See §5)

## Phase 3: Frontend event hub
- [x] Add `agent:stream_event` hub in `app/src/services/events.ts` (See §4)
- [x] Update UI reducers to consume `StreamEvent` (See §5)

Exit Criteria
- [ ] UI renders assistant text via `agent:stream_event` (manual)
- [x] `seq` gaps are handled without crashing the reducer (See §5)

## Phase 4: Deprecate legacy events
- [ ] Remove OpenCode-specific stream events after migration (See §9)
- [ ] Add compatibility adapter if legacy events remain (See §9)

Exit Criteria
- [ ] No references to legacy stream events in frontend hooks (See §9)
- [ ] `agent:stream_event` is the only streaming event path (See §9)

## Files to Create
- `app/src/types/streaming.ts`
- `app/src-tauri/src/daemon/opencode_adapter.rs`
- `app/src-tauri/src/daemon/claudecode_adapter.rs`

## Files to Modify
- `app/src-tauri/src/sessions.rs`
- `app/src-tauri/src/lib.rs`
- `app/src-tauri/src/daemon/mod.rs`
- `app/src-tauri/src/daemon/client.rs`
- `app/src/services/events.ts`
- `app/src/features/opencode/hooks/useOpenCodeThread.ts`

## Verification Checklist
- [x] `bun run typecheck`
- [ ] Manual: verify ordered text/tool deltas in UI
