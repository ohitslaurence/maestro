# Task: Implement Tauri IPC Wrapper Pattern

## Objective

Port CodexMonitor's Tauri IPC wrapper pattern to Maestro. This provides type-safe, function-per-command frontend-to-backend communication.

## Reference

- `reference/codex-monitor/src/services/tauri.ts` - Frontend IPC wrapper
- `reference/codex-monitor/src-tauri/src/lib.rs` - Command handlers

## Output

Create `app/src/services/tauri.ts` with:
- Type-safe invoke wrappers for each Tauri command
- Consistent error handling pattern
- TypeScript types matching Rust return types

## Implementation Details

### Pattern to Copy

```typescript
// Each command gets a typed wrapper function
export async function listWorkspaces(): Promise<WorkspaceInfo[]> {
  return invoke("list_workspaces");
}

export async function getGitStatus(workspaceId: string): Promise<GitFileStatus[]> {
  return invoke("get_git_status", { workspaceId });
}
```

### Key Points

1. One function per Tauri command
2. TypeScript types mirror Rust structs
3. Errors propagate as rejected promises (string messages)
4. No business logic in service layer - just IPC

### Commands to Implement (Initial Set)

- Session lifecycle: `list_sessions`, `spawn_session`, `stop_session`
- Terminal: `terminal_open`, `terminal_write`, `terminal_resize`, `terminal_close`
- Git: `get_git_status`, `get_git_diffs`, `get_git_log`

## Constraints

- Keep service layer thin - no state, no side effects
- Match CodexMonitor's naming conventions where applicable
- Document each function with JSDoc

## Dependencies

- Tauri backend commands must exist first (or stub them)
- TypeScript types in `app/src/types.ts`
