# Dynamic Tool Approvals Implementation Plan

Reference: [dynamic-tool-approvals.md](../dynamic-tool-approvals.md)

## Phase 1: Permission Manager Infrastructure

- [x] Create `PermissionRequest`, `PermissionReplyRequest`, `PendingPermission` types (§3)
- [x] Create `PermissionManager` class with pending request storage (§4)
- [x] Implement `request()` method that returns blocking Promise (§4)
- [x] Implement `reply()` method that resolves/rejects pending Promise (§4)
- [x] Implement `isApproved()` for "always" pattern checking with exact string match (§4)
- [x] Implement `findSessionForRequest()` with O(1) reverse lookup map (§4)
- [x] Implement `clearSession()` for cleanup (§4)
- [x] Add timeout handling for stale permissions (§6)

## Phase 2: Update canUseTool Handler [BLOCKED by: Phase 1]

- [ ] Refactor `createCanUseTool` to use `PermissionManager` (§4)
- [ ] Add `getMessageId` callback parameter for message ID population (§4)
- [ ] Implement `buildPermissionRequest()` helper (§4)
- [ ] Implement `extractPatterns()` for each tool type (§3)
- [ ] Implement `extractMetadata()` for tool-specific context (§3)
- [ ] Handle abort signal in permission flow (§5)
- [ ] Auto-approve safe tools, block dangerous tools for approval (§4)

## Phase 3: Permission Reply Endpoint [BLOCKED by: Phase 1]

- [ ] Create `routes/permissions.ts` with Hono router (§4)
- [ ] Implement `POST /permission/:requestId/reply` endpoint (§4)
- [ ] Implement `GET /permission/pending` for reconnection (§4, §5)
- [ ] Mount routes in server index
- [ ] Use `findSessionForRequest()` for O(1) session lookup (§4)

## Phase 4: Enhanced SSE Events [BLOCKED by: Phase 1, Phase 2]

- [ ] Update `PermissionAskedEvent` to include full `PermissionRequest` (§4)
- [ ] Ensure `permission.replied` events include reply type (§4)
- [ ] Emit events from `PermissionManager` methods (§4)

## Phase 5: Tauri Commands [BLOCKED by: Phase 3]

- [ ] Add `claude_sdk_permission_reply` command
- [ ] Add `claude_sdk_permission_pending` command
- [ ] Update Rust types for permission events

## Phase 6: Frontend Service Layer [BLOCKED by: Phase 5]

- [ ] Add `claudeSdkPermissionReply` service function
- [ ] Add `claudeSdkPermissionPending` service function
- [ ] Add TypeScript types for `PermissionRequest` (§3)

## Phase 7: UI Components [BLOCKED by: Phase 6]

- [ ] Create `PermissionModal.tsx` component (§UI Components)
- [ ] Create `PermissionContext.tsx` for tool-specific rendering (§UI Components)
- [ ] Implement Edit context with diff viewer (§UI Components)
- [ ] Implement Bash context with command display (§UI Components)
- [ ] Implement Write/Read context with file path (§UI Components)
- [ ] Implement WebFetch context with URL (§UI Components)
- [ ] Style modal with existing design tokens

## Phase 8: usePermissions Hook [BLOCKED by: Phase 7]

- [ ] Create `usePermissions` hook with queue-based state (§UI Components, §5)
- [ ] Subscribe to `permission.asked` SSE events (§UI Components)
- [ ] Implement `reply()` callback (§UI Components)
- [ ] Implement `dismiss()` for deny with default message (§UI Components)
- [ ] Handle pending permission fetch on reconnect (§5)
- [ ] Support concurrent permissions via queue (§5)

## Phase 9: Thread View Integration [BLOCKED by: Phase 8]

- [ ] Add `PermissionModal` to `ClaudeThreadView`
- [ ] Disable composer input while permission pending
- [ ] Show visual indicator when awaiting approval

## Files to Create

- `daemon/claude-server/src/permissions/manager.ts`
- `daemon/claude-server/src/permissions/types.ts`
- `daemon/claude-server/src/routes/permissions.ts`
- `app/src/features/claudecode/components/PermissionModal.tsx`
- `app/src/features/claudecode/components/PermissionContext.tsx`
- `app/src/features/claudecode/hooks/usePermissions.ts`

## Files to Modify

- `daemon/claude-server/src/types.ts`
- `daemon/claude-server/src/sdk/permissions.ts`
- `daemon/claude-server/src/events/emitter.ts`
- `daemon/claude-server/src/index.ts`
- `app/src-tauri/src/lib.rs`
- `app/src/services/tauri.ts`
- `app/src/features/claudecode/components/ClaudeThreadView.tsx`

## Verification Checklist

### Implementation Checklist

- [ ] `cd daemon/claude-server && bun run typecheck`
- [ ] `cd app && bun run typecheck`
- [ ] Permission request blocks SDK execution
- [ ] Reply endpoint resolves pending permission
- [ ] "Always" reply adds pattern to approved set (exact match)
- [ ] Abort cancels pending permissions
- [ ] Timeout rejects stale permissions
- [ ] Concurrent permissions queue correctly in UI

### Manual QA Checklist (do not mark—human verification)

- [ ]? Permission modal appears for dangerous tools (Bash, Edit, Write)
- [ ]? Modal shows appropriate context (command, diff, file path)
- [ ]? "Allow Once" proceeds and completes tool
- [ ]? "Deny" stops tool execution with message
- [ ]? "Always Allow" prevents future prompts for same exact pattern
- [ ]? Reconnecting client sees pending permission
- [ ]? Multiple concurrent permissions show sequentially

### UI Feature Validation

- [ ] `cd daemon && cargo run -- --listen 127.0.0.1:55433 --insecure-no-auth`
- [ ] `cd app && bun run dev -- --host 127.0.0.1 --port 1420`
- [ ] `cd app && bun scripts/ui-permissions.ts`

## Notes

- Phase 1: Use `crypto.randomUUID()` for request IDs. Store in `Map<sessionId, Map<requestId, PendingPermission>>` with reverse `Map<requestId, sessionId>` for O(1) lookup.
- Phase 2: Dangerous tools: `['Write', 'Edit', 'Bash', 'WebFetch', 'WebSearch']`. Safe tools auto-approve. `getMessageId` callback solves the messageId population issue from the original spec.
- Phase 4: The enhanced `PermissionAskedEvent` includes full request object, not just ID and tool name.
- Phase 7: Diff viewer can reuse existing `DiffViewer` component from git feature if available.
- Phase 8: Hook uses queue (`pendingQueue`) not single state to handle concurrent tool permissions.
- Phase 9: Consider adding a "permission pending" indicator in the thread (e.g., pulsing badge).
- Pattern matching: Uses exact string equality, not glob patterns. Clarified in §3 and §10.
- Session cleanup: `clearSession()` called when session ends or disconnects (§4).
