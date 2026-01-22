# Claude Session History and Resume UI Implementation Plan

Reference: [claude-session-history.md](../claude-session-history.md)

## Phase 1: Claude server history endpoint
- [x] Add `GET /session/:id/message?limit=N` (default 100, max 500) to return message history with parts (See §4)
- [x] Return messages in chronological ascending order by `time.created` (See §3 Ordering)
- [x] Include all parts per message in insertion order (See §3 Ordering)

## Phase 2: Daemon and Tauri wiring [BLOCKED by: Phase 1]
- [x] Add `claude_sdk_session_messages` RPC handler and protocol constant (See §4)
- [x] Add `claudeSdkSessionMessages` wrapper in `app/src/services/tauri.ts` (See §4)

## Phase 3: UI session list and resume flow [BLOCKED by: Phase 2]
- [x] Add `useClaudeSessions` hook to fetch list + selection state (See §2, §5)
- [x] Add `ClaudeSessionList` UI and wire selection into `ClaudeThreadView` (See §2, §5)
- [x] Update `useOpenCodeThread` to replace thread state on history load; preserve composer draft (See §5 Main Flow)
- [x] Implement AbortController for concurrent session selection (See §5 Concurrent Selection)
- [x] Handle edge cases: empty session, external deletion (See §5 Edge Cases)

## Files to Create
- `app/src/features/claudecode/components/ClaudeSessionList.tsx`
- `app/src/features/claudecode/hooks/useClaudeSessions.ts`

## Files to Modify
- `daemon/claude-server/src/server.ts`
- `daemon/src/handlers/claude_sdk.rs`
- `daemon/src/protocol.rs`
- `app/src/services/tauri.ts`
- `app/src/features/claudecode/components/ClaudeThreadView.tsx`
- `app/src/features/claudecode/hooks/useClaudeSession.ts`
- `app/src/features/opencode/hooks/useOpenCodeThread.ts`

## Verification Checklist
### Implementation Checklist
- [x] `cd app && bun run typecheck` (only pre-existing diffsWorker.ts error)

### Manual QA Checklist (do not mark—human verification)
- [ ]? UI Feature Validation:
  - [ ] `cd daemon && cargo run -- --listen 127.0.0.1:55433 --insecure-no-auth`
  - [ ] `cd app && bun run dev -- --host 127.0.0.1 --port 1420`
  - [ ] `cd app && bun scripts/ui-claude-session-history.ts`
