# Dynamic Tool Approvals Implementation Plan

Reference: [dynamic-tool-approvals.md](../dynamic-tool-approvals.md)

## Phase 1: Permission Manager Infrastructure

- [ ] Create `PermissionRequest` and `PermissionReply` types (§1)
- [ ] Create `PermissionManager` class with pending request storage (§2)
- [ ] Implement `request()` method that returns blocking Promise (§2)
- [ ] Implement `reply()` method that resolves/rejects pending Promise (§2)
- [ ] Implement `isApproved()` for "always" pattern checking (§2)
- [ ] Implement `clearSession()` for cleanup (§2)
- [ ] Add timeout handling for stale permissions (§Timeout)

## Phase 2: Update canUseTool Handler [BLOCKED by: Phase 1]

- [ ] Refactor `createCanUseTool` to use `PermissionManager` (§2)
- [ ] Implement `buildPermissionRequest()` helper (§2)
- [ ] Implement `extractPatterns()` for each tool type (§2)
- [ ] Implement `extractMetadata()` for tool-specific context (§2)
- [ ] Handle abort signal in permission flow (§2)
- [ ] Auto-approve safe tools, block dangerous tools for approval (§2)

## Phase 3: Permission Reply Endpoint [BLOCKED by: Phase 1]

- [ ] Create `routes/permissions.ts` with Hono router (§2)
- [ ] Implement `POST /permission/:requestId/reply` endpoint (§2)
- [ ] Implement `GET /permission/pending` for reconnection (§2)
- [ ] Mount routes in server index (§2)
- [ ] Add session lookup for request ID

## Phase 4: Enhanced SSE Events [BLOCKED by: Phase 2]

- [ ] Update `PermissionAskedEvent` to include full `PermissionRequest` (§3)
- [ ] Ensure `permission.replied` events include reply type (§3)
- [ ] Emit events from `PermissionManager` methods (§2)

## Phase 5: Tauri Commands [BLOCKED by: Phase 3]

- [ ] Add `claude_sdk_permission_reply` command
- [ ] Add `claude_sdk_permission_pending` command
- [ ] Update Rust types for permission events

## Phase 6: Frontend Service Layer [BLOCKED by: Phase 5]

- [ ] Add `claudeSdkPermissionReply` service function
- [ ] Add `claudeSdkPermissionPending` service function
- [ ] Add TypeScript types for `PermissionRequest`

## Phase 7: UI Components [BLOCKED by: Phase 6]

- [ ] Create `PermissionModal.tsx` component (§4)
- [ ] Create `PermissionContext.tsx` for tool-specific rendering (§4)
- [ ] Implement Edit context with diff viewer (§4)
- [ ] Implement Bash context with command display (§4)
- [ ] Implement Write/Read context with file path (§4)
- [ ] Implement WebFetch context with URL (§4)
- [ ] Style modal with existing design tokens

## Phase 8: usePermissions Hook [BLOCKED by: Phase 7]

- [ ] Create `usePermissions` hook (§4)
- [ ] Subscribe to `permission.asked` SSE events (§4)
- [ ] Implement `reply()` callback (§4)
- [ ] Implement `dismiss()` for deny with default message (§4)
- [ ] Handle pending permission on reconnect (§4)

## Phase 9: Thread View Integration [BLOCKED by: Phase 8]

- [ ] Add `PermissionModal` to `ClaudeThreadView` (§5)
- [ ] Disable composer input while permission pending (§5)
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
- [ ] "Always" reply adds pattern to approved set
- [ ] Abort cancels pending permissions
- [ ] Timeout rejects stale permissions

### Manual QA Checklist (do not mark—human verification)

- [ ]? Permission modal appears for dangerous tools (Bash, Edit, Write)
- [ ]? Modal shows appropriate context (command, diff, file path)
- [ ]? "Allow Once" proceeds and completes tool
- [ ]? "Deny" stops tool execution with message
- [ ]? "Always Allow" prevents future prompts for same pattern
- [ ]? Reconnecting client sees pending permission

## Notes

- Phase 1: Use `crypto.randomUUID()` for request IDs. Store in `Map<sessionId, Map<requestId, PendingPermission>>`.
- Phase 2: Dangerous tools list: `['Write', 'Edit', 'Bash', 'WebFetch', 'WebSearch']`. Safe tools auto-approve.
- Phase 4: The enhanced `PermissionAskedEvent` includes full request object, not just ID and tool name.
- Phase 7: Diff viewer can reuse existing `DiffViewer` component from git feature if available.
- Phase 9: Consider adding a "permission pending" indicator in the thread (e.g., pulsing badge).
