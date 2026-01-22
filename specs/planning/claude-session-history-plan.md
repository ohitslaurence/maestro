# Claude Session History and Resume UI Implementation Plan

Reference: [claude-session-history.md](../claude-session-history.md)

## Phase 1: Claude server history endpoint
- [ ] Add `GET /session/:id/message` to return message history with parts (See §4)
- [ ] Ensure history payload aligns with `ClaudeMessageInfo` ordering (See §3, §5)

## Phase 2: Daemon and Tauri wiring [BLOCKED by: Phase 1]
- [ ] Add `claude_sdk_session_messages` RPC handler and protocol constant (See §4)
- [ ] Add `claudeSdkSessionMessages` wrapper in `app/src/services/tauri.ts` (See §4)

## Phase 3: UI session list and resume flow [BLOCKED by: Phase 2]
- [ ] Add `useClaudeSessions` hook to fetch list + selection state (See §2, §5)
- [ ] Add `ClaudeSessionList` UI and wire selection into `ClaudeThreadView` (See §2, §5)
- [ ] Update `useOpenCodeThread` history loader to use Claude history API (See §5)

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
- [ ] `cd app && bun run typecheck`

### Manual QA Checklist (do not mark—human verification)
- [ ]? UI Feature Validation:
  - [ ] `cd daemon && cargo run -- --listen 127.0.0.1:55433 --insecure-no-auth`
  - [ ] `cd app && bun run dev -- --host 127.0.0.1 --port 1420`
  - [ ] `cd app && bun scripts/ui-claude-session-history.ts`
