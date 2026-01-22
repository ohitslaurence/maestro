# Claude SDK Server Restart Resilience Implementation Plan

Reference: [claude-sdk-restart-resilience.md](../claude-sdk-restart-resilience.md)

## Phase 1: Runtime tracking and port assignment
- [x] Add `ClaudeServerRuntime` struct with `port`, `base_url`, `restart_count`, `status` to `DaemonState` (See §3)
- [x] Allocate an available port on initial spawn and pass via `MAESTRO_PORT` (See §5 step 1)
- [x] Add `/health` endpoint to `daemon/claude-server/src/server.ts` returning 200 OK (See §5 step 3)

## Phase 2: Restart behavior updates [BLOCKED by: Phase 1]
- [x] Implement health-check polling (100ms interval, 30s timeout) to transition `Starting` → `Ready` (See §5 step 3)
- [x] On unexpected exit, set status to `Starting`, wait 1s, respawn with same port (See §5 step 4)
- [ ] On EADDRINUSE, allocate new port and update `base_url` (See §5 Edge Cases)
- [x] Restart SSE bridge when `base_url` changes (See §5 step 5)

## Phase 3: Failure handling and logging [BLOCKED by: Phase 2]
- [ ] After 2 consecutive failures, set status to `Error` and stop auto-retry (See §5 step 6, §6)
- [ ] Log port allocation, health-check results, and restart attempts with workspace ID (See §7)
- [ ] Ensure `claude_sdk_status` returns current `status` and `base_url` (See §4)

## Files to Create
- None

## Files to Modify
- `daemon/src/claude_sdk.rs`
- `daemon/src/state.rs`
- `daemon/src/handlers/claude_sdk.rs`
- `daemon/claude-server/src/server.ts`

## Verification Checklist
### Implementation Checklist
- [ ] `cd daemon && cargo build`

### Manual QA Checklist (do not mark—human verification)
- [ ]? Manually kill Claude server process and verify daemon restarts and updates `base_url`
