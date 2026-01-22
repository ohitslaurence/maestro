# Session Settings Spec

**Status:** In Progress
**Version:** 1.0
**Last Updated:** 2026-01-22

---

## 1. Overview

### Purpose

Expose configuration options for Claude SDK sessions beyond the current minimal set (title, modelId, permission). Users need control over sampling parameters, iteration limits, system prompts, and tool configuration.

### Goals

- Give users the same control they have with Claude Code CLI flags, but through the UI
- Session-level settings that override workspace defaults
- Settings persist across session resume and server restart

### Non-Goals

- Per-agent named configurations (OpenCode pattern)
- MCP server configuration (deferred to future phase)
- System prompt presets (deferred; use append/custom modes only)

### References

| File | Purpose |
|------|---------|
| `daemon/claude-server/src/types.ts` | Current `Session` and `CreateSessionRequest` interfaces to extend |
| `daemon/claude-server/src/sdk/agent.ts` | `buildSdkOptions()` function to wire settings into SDK calls |
| `daemon/claude-server/src/routes/sessions.ts` | Existing session routes; add PATCH endpoint here |
| `daemon/claude-server/src/storage/sessions.ts` | Session storage implementation to persist settings |
| `daemon/claude-server/src/index.ts` | Server entry point for route registration |
| `app/src/services/tauri.ts` | Tauri service layer to add settings commands |
| `app/src-tauri/src/lib.rs` | Rust Tauri commands for `claude_sdk_session_settings_update` |
| `app/src/features/claudecode/components/ClaudeThreadView.tsx` | Session header where settings button will be added |

---

## 2. Architecture

### Components

```
┌─────────────────────────────────────────────────────────────────┐
│                         Frontend (React)                         │
├─────────────────────────────────────────────────────────────────┤
│  SessionSettingsButton → SessionSettingsModal                   │
│       ↓                        ↓                                │
│  useSessionSettings ←→ claudeSdkSessionSettingsUpdate()         │
└─────────────────────────────────────────────────────────────────┘
                              ↓ Tauri IPC
┌─────────────────────────────────────────────────────────────────┐
│                      Tauri (Rust)                               │
├─────────────────────────────────────────────────────────────────┤
│  claude_sdk_session_settings_update command                     │
└─────────────────────────────────────────────────────────────────┘
                              ↓ HTTP
┌─────────────────────────────────────────────────────────────────┐
│                   Claude Server (Bun)                           │
├─────────────────────────────────────────────────────────────────┤
│  PATCH /session/:id/settings → SessionStorage → SDK options     │
└─────────────────────────────────────────────────────────────────┘
```

### Dependencies

- Claude SDK: `@anthropic-ai/claude-code` for `customSystemPrompt`, `appendSystemPrompt`, `allowedTools`, `disallowedTools`
- Existing session management infrastructure

### Module/Folder Layout

```
daemon/claude-server/src/
├── types.ts              # SessionSettings type additions
├── routes/sessions.ts    # PATCH endpoint
├── sdk/agent.ts          # buildSdkOptions() updates
└── storage/sessions.ts   # Settings persistence

app/src/features/claudecode/
├── components/
│   ├── SessionSettingsModal.tsx
│   ├── SessionSettingsButton.tsx
│   ├── SystemPromptEditor.tsx
│   └── ToolSelector.tsx
└── hooks/
    └── useSessionSettings.ts
```

---

## 3. Data Model

### Core Types

```typescript
// daemon/claude-server/src/types.ts

interface Session {
  // Existing fields
  id: string;
  workspaceId: string;
  directory: string;
  title: string;
  parentId?: string;
  resumeId?: string;
  modelId?: string;
  createdAt: number;
  updatedAt: number;
  status: SessionStatus;
  permission: PermissionMode;
  summary?: string;

  // NEW: Extended settings (always present, defaults applied)
  settings: SessionSettings;
}

interface SessionSettings {
  maxTurns: number;                    // Default: 100, Range: 1-1000
  systemPrompt: SystemPromptConfig;    // Default: { mode: 'default' }
  disallowedTools?: string[];          // Blocklist (removed from defaults)
}

// Simplified from union type for clarity
interface SystemPromptConfig {
  mode: 'default' | 'append' | 'custom';
  content?: string;  // Required for 'append' and 'custom' modes
}

// Default settings applied when not provided
const DEFAULT_SESSION_SETTINGS: SessionSettings = {
  maxTurns: 100,
  systemPrompt: { mode: 'default' },
  disallowedTools: undefined,
};
```

**Design decisions:**
- `disallowedTools` only (no `allowedTools`): Blocklist-only simplifies the model. Most sessions want all tools; allowlist is rarely useful.
- `SystemPromptConfig` uses a single interface with mode field instead of union type for easier validation.
- Empty array `disallowedTools: []` is equivalent to `undefined` (no tools blocked).

### Storage Schema

Settings are stored inline with the session in SQLite. No separate table needed.

```sql
-- Sessions table already exists; settings stored as JSON in settings column
ALTER TABLE sessions ADD COLUMN settings TEXT DEFAULT '{}';
```

---

## 4. Interfaces

### Public APIs

#### PATCH /session/:id/settings

Update session settings. Partial updates supported.

**Request:**
```typescript
interface UpdateSessionSettingsRequest {
  settings: Partial<SessionSettings>;
}
```

**Merge semantics:**
- `undefined` field = leave unchanged
- `null` field = reset to default
- Provided value = set to value

**Validation:**
- `maxTurns`: Must be integer 1-1000
- `systemPrompt.content`: Required when mode is 'append' or 'custom'
- `disallowedTools`: Array of strings; invalid tool names are silently ignored (SDK handles unknown tools)

**Example request:**
```json
{
  "settings": {
    "maxTurns": 50,
    "systemPrompt": {
      "mode": "append",
      "content": "Always use TypeScript."
    },
    "disallowedTools": ["WebSearch"]
  }
}
```

**Response:** Updated `Session` object with merged settings.

**Error responses:**
- `400 Bad Request`: Invalid settings (maxTurns out of range, missing content for append/custom)
- `404 Not Found`: Session not found

#### GET /session/:id

Now includes settings in response:

```json
{
  "id": "session-123",
  "title": "My Session",
  "settings": {
    "maxTurns": 100,
    "systemPrompt": { "mode": "default" }
  }
}
```

#### POST /session (extended)

```typescript
interface CreateSessionRequest {
  title: string;
  parentId?: string;
  permission?: PermissionMode;
  modelId?: string;
  settings?: Partial<SessionSettings>;  // NEW: Optional initial settings
}
```

When `parentId` is set (fork), child session inherits parent's settings unless overridden in request.

### Internal APIs

```typescript
// daemon/claude-server/src/sdk/agent.ts
function buildSdkOptions(
  session: Session,
  messageOverrides?: MessageOverrides,
  resumeId?: string,
  abortController?: AbortController
): SdkOptions {
  const settings = session.settings;

  return {
    cwd: session.directory,
    permissionMode: mapPermissionMode(session.permission),
    model: messageOverrides?.modelId ?? session.modelId,
    maxThinkingTokens: messageOverrides?.maxThinkingTokens,
    abortController,
    resume: resumeId,
    canUseTool: createCanUseTool(session.id, session.permission),
    hooks: buildHooksConfig(session.id),

    // From session settings
    maxTurns: settings.maxTurns,
    ...buildSystemPromptOptions(settings.systemPrompt),
    ...(settings.disallowedTools?.length && { disallowedTools: settings.disallowedTools }),
  };
}

function buildSystemPromptOptions(config: SystemPromptConfig): Record<string, string> {
  switch (config.mode) {
    case 'default':
      return {};
    case 'append':
      return { appendSystemPrompt: config.content! };
    case 'custom':
      return { customSystemPrompt: config.content! };
  }
}
```

### Events

**SSE event on settings change:**

```typescript
// Emitted when PATCH /session/:id/settings succeeds
interface SessionUpdatedEvent {
  type: 'session.updated';
  sessionId: string;
  session: Session;  // Full session object with new settings
}
```

---

## 5. Workflows

### Main Flow: Update Settings

```
User clicks gear icon in session header
    ↓
SessionSettingsModal opens with current settings
    ↓
User modifies settings (maxTurns, systemPrompt, tools)
    ↓
User clicks "Save Settings"
    ↓
useSessionSettings.updateSettings(patch)
    ↓
Optimistic UI update
    ↓
PATCH /session/:id/settings
    ↓
Server validates and merges settings
    ↓
Server persists to storage
    ↓
Server emits session.updated SSE event
    ↓
Response returns updated session
    ↓
UI confirms save (or rolls back on error)
```

### Edge Cases

**Settings update during active turn:**
- Settings are read at turn start. Changes mid-turn apply to the *next* turn.
- No interruption of current agent execution.

**Concurrent PATCH requests:**
- Last-write-wins. No optimistic locking.
- Server merges each request independently against current state.

**Session fork (parentId set):**
- Child inherits parent's settings at fork time.
- Subsequent parent changes do not affect child.

---

## 6. Error Handling

### Error Types

| Error | HTTP Status | Cause |
|-------|-------------|-------|
| `INVALID_MAX_TURNS` | 400 | maxTurns outside 1-1000 range |
| `MISSING_PROMPT_CONTENT` | 400 | systemPrompt mode is append/custom but content is empty |
| `SESSION_NOT_FOUND` | 404 | Session ID does not exist |
| `STORAGE_ERROR` | 500 | Failed to persist settings |

### Recovery Strategy

- Validation errors: Return immediately with error details; no state change.
- Storage errors: Return 500; client should retry with exponential backoff.
- UI: Roll back optimistic update on any error; show toast with error message.

---

## 7. Observability

### Logs

```
[INFO] Session settings updated: sessionId=abc123 maxTurns=50 systemPromptMode=append
[WARN] Invalid tool in disallowedTools: toolName=FakeToolXYZ sessionId=abc123
[ERROR] Failed to persist session settings: sessionId=abc123 error=...
```

### Metrics

- `session_settings_updates_total`: Counter of settings update requests
- `session_settings_validation_errors_total`: Counter by error type

### Traces

No additional tracing beyond existing session request traces.

---

## 8. Security and Privacy

### AuthZ/AuthN

- Settings endpoint uses same auth as existing session endpoints.
- No additional permissions required; user can modify settings for any session they can access.

### Data Handling

- System prompt content may contain sensitive instructions; stored encrypted with session data.
- No PII in settings fields.

---

## 9. Migration or Rollout

### Compatibility Notes

- Existing sessions have no `settings` field. On read, apply `DEFAULT_SESSION_SETTINGS`.
- No database migration required if using lazy initialization.
- Alternative: Run migration to add `settings` column with default JSON.

### Rollout Plan

1. Deploy server with new endpoint (backward compatible; settings optional).
2. Deploy UI with settings modal.
3. (Optional) Run migration to backfill settings column.

---

## 10. Open Questions

1. **Workspace defaults**: Should workspaces have default settings that sessions inherit? (Deferred)

2. **Settings templates**: Should users be able to save/load settings presets? (Deferred)

3. **Per-message overrides**: Should settings be overridable per-message (like model/thinking in composer-options)? (Deferred)

---

## Appendix: UI Components

### SessionSettingsModal

```
┌─────────────────────────────────────────────────────────┐
│  Session Settings                              [×]      │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌─ Execution ────────────────────────────────────────┐ │
│  │                                                     │ │
│  │  Max Turns    [100        ]  (1-1000)              │ │
│  │                                                     │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                         │
│  ┌─ System Prompt ────────────────────────────────────┐ │
│  │                                                     │ │
│  │  Mode  [● Default] [○ Append] [○ Custom]           │ │
│  │                                                     │ │
│  │  ┌─────────────────────────────────────────────┐   │ │
│  │  │ Additional instructions...                   │   │ │
│  │  │                                              │   │ │
│  │  └─────────────────────────────────────────────┘   │ │
│  │                                                     │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                         │
│  ┌─ Tools ────────────────────────────────────────────┐ │
│  │                                                     │ │
│  │  Disable specific tools:                           │ │
│  │  [ ] Bash    [ ] WebFetch   [ ] WebSearch          │ │
│  │                                                     │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                         │
│                              [Cancel]  [Save Settings]  │
└─────────────────────────────────────────────────────────┘
```

### Tool List

Available tools for blocklist configuration:

```typescript
const CLAUDE_TOOLS = [
  { id: 'Read', name: 'Read', description: 'Read file contents', category: 'files' },
  { id: 'Write', name: 'Write', description: 'Write file contents', category: 'files' },
  { id: 'Edit', name: 'Edit', description: 'Edit file contents', category: 'files' },
  { id: 'Glob', name: 'Glob', description: 'Find files by pattern', category: 'files' },
  { id: 'Grep', name: 'Grep', description: 'Search file contents', category: 'files' },
  { id: 'Bash', name: 'Bash', description: 'Run shell commands', category: 'system' },
  { id: 'Task', name: 'Task', description: 'Spawn subagents', category: 'agents' },
  { id: 'TodoWrite', name: 'TodoWrite', description: 'Track tasks', category: 'agents' },
  { id: 'WebFetch', name: 'WebFetch', description: 'Fetch web pages', category: 'web' },
  { id: 'WebSearch', name: 'WebSearch', description: 'Search the web', category: 'web' },
] as const;
```
