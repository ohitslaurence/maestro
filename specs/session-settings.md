# Session Settings Spec

## Overview

Expose configuration options for Claude SDK sessions beyond the current minimal set (title, modelId, permission). Users need control over sampling parameters, iteration limits, system prompts, and tool configuration.

**Goal**: Give users the same control they have with Claude Code CLI flags, but through the UI.

## Current State

### Session Fields (Maestro)

```typescript
// daemon/claude-server/src/types.ts
interface Session {
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
  permission: PermissionMode;  // default | acceptEdits | bypassPermissions
  summary?: string;
}

interface CreateSessionRequest {
  title: string;
  parentId?: string;
  permission?: PermissionMode;
  modelId?: string;
}
```

### SDK Options (hardcoded)

```typescript
// daemon/claude-server/src/sdk/agent.ts
return {
  cwd: session.directory,
  permissionMode: session.permission,
  maxTurns: 100,  // Hardcoded!
  model: session.modelId,
  // No temperature, topP, systemPrompt, tools, etc.
};
```

### What's Missing

| Setting | CLI Flag | SDK Option | Maestro |
|---------|----------|------------|---------|
| Temperature | N/A | N/A (SDK handles) | Not exposed |
| Max turns | `--max-turns` | `maxTurns` | Hardcoded 100 |
| System prompt | `--system-prompt` | `customSystemPrompt` | Not exposed |
| Append prompt | `--append-system-prompt` | `appendSystemPrompt` | Not exposed |
| Allowed tools | `--allowedTools` | `allowedTools` | Not exposed |
| Disallowed tools | `--disallowedTools` | `disallowedTools` | Not exposed |
| MCP servers | `--mcp-config` | `mcpServers` | Not exposed |

## OpenCode Reference

OpenCode uses hierarchical config with per-agent settings:

```typescript
// external/opencode/packages/opencode/src/agent/agent.ts
Agent.Info {
  name: string;
  model?: { providerID, modelID };
  prompt?: string;           // System prompt
  temperature?: number;
  topP?: number;
  steps?: number;            // Max iterations
  permission: Ruleset;       // Fine-grained permissions
  options: Record<string, unknown>;
}
```

Key insight: OpenCode separates **agents** (named configurations) from **sessions** (runtime instances). We can adopt a simpler model: session-level settings that override defaults.

## Design

### 1. Extended Session Model

```typescript
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

  // NEW: Extended settings
  settings: SessionSettings;
}

interface SessionSettings {
  // Sampling
  maxTurns?: number;              // Default: 100

  // System prompt
  systemPrompt?: SystemPromptConfig;

  // Tools
  allowedTools?: string[];        // Whitelist (if set, only these)
  disallowedTools?: string[];     // Blacklist (removed from defaults)

  // MCP (future)
  mcpServers?: McpServerConfig[];
}

type SystemPromptConfig =
  | { type: 'default' }                        // Use SDK default
  | { type: 'preset'; preset: string }         // Named preset (claude_code, etc.)
  | { type: 'custom'; prompt: string }         // Full custom prompt
  | { type: 'append'; append: string };        // Append to default

interface McpServerConfig {
  name: string;
  type: 'stdio' | 'sse' | 'http';
  command?: string;
  args?: string[];
  url?: string;
  headers?: Record<string, string>;
}
```

### 2. Create Session Request (extended)

```typescript
interface CreateSessionRequest {
  title: string;
  parentId?: string;
  permission?: PermissionMode;
  modelId?: string;

  // NEW
  settings?: Partial<SessionSettings>;
}
```

### 3. Update Session Settings Endpoint (new)

```typescript
// PATCH /session/:id/settings
interface UpdateSessionSettingsRequest {
  settings: Partial<SessionSettings>;
}

// Response: updated Session
```

### 4. SDK Options Builder (updated)

```typescript
// daemon/claude-server/src/sdk/agent.ts
function buildSdkOptions(
  session: Session,
  messageOverrides?: MessageOverrides,
  resumeId?: string,
  abortController?: AbortController
) {
  const settings = session.settings || {};

  return {
    cwd: session.directory,
    permissionMode: mapPermissionMode(session.permission),
    model: messageOverrides?.modelId ?? session.modelId,
    maxThinkingTokens: messageOverrides?.maxThinkingTokens,
    abortController,
    resume: resumeId,
    canUseTool: createCanUseTool(session.id, session.permission),
    hooks: buildHooksConfig(session.id),

    // NEW: From session settings
    maxTurns: settings.maxTurns ?? 100,
    ...buildSystemPromptOptions(settings.systemPrompt),
    ...buildToolOptions(settings.allowedTools, settings.disallowedTools),
    ...(settings.mcpServers && { mcpServers: buildMcpConfig(settings.mcpServers) }),
  };
}

function buildSystemPromptOptions(config?: SystemPromptConfig) {
  if (!config || config.type === 'default') {
    return {};
  }
  if (config.type === 'preset') {
    // SDK doesn't have preset support yet, use custom
    return { customSystemPrompt: PRESETS[config.preset] };
  }
  if (config.type === 'custom') {
    return { customSystemPrompt: config.prompt };
  }
  if (config.type === 'append') {
    return { appendSystemPrompt: config.append };
  }
  return {};
}

function buildToolOptions(allowed?: string[], disallowed?: string[]) {
  const opts: Record<string, unknown> = {};
  if (allowed?.length) {
    opts.allowedTools = allowed;
  }
  if (disallowed?.length) {
    opts.disallowedTools = disallowed;
  }
  return opts;
}
```

### 5. UI Components

#### SessionSettingsModal

Modal for configuring session settings, accessible from:
- Session header (gear icon)
- New session dialog (advanced section)

```
┌─────────────────────────────────────────────────────────┐
│  Session Settings                              [×]      │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  ┌─ Execution ────────────────────────────────────────┐ │
│  │                                                     │ │
│  │  Max Turns    [100        ]  (iterations before    │ │
│  │                               stopping)            │ │
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
│  │  [✓] Read    [✓] Write   [✓] Edit                  │ │
│  │  [✓] Bash    [✓] Glob    [✓] Grep                  │ │
│  │  [✓] WebFetch [✓] WebSearch [✓] Task               │ │
│  │                                                     │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                         │
│                              [Cancel]  [Save Settings]  │
└─────────────────────────────────────────────────────────┘
```

#### SessionSettingsButton

Small gear icon in session header:

```tsx
interface SessionSettingsButtonProps {
  session: Session;
  onOpenSettings: () => void;
}
```

#### useSessionSettings Hook

```typescript
interface UseSessionSettingsReturn {
  settings: SessionSettings;
  updateSettings: (patch: Partial<SessionSettings>) => Promise<void>;
  isUpdating: boolean;
  error: string | null;
}

function useSessionSettings(sessionId: string): UseSessionSettingsReturn {
  // Fetch current settings
  // Provide update function that PATCHes /session/:id/settings
  // Optimistic updates with rollback on error
}
```

### 6. Tool Configuration

#### Available Tools

From Claude SDK, the built-in tools are:

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

#### Tool Selector Component

```tsx
interface ToolSelectorProps {
  allowedTools?: string[];
  disallowedTools?: string[];
  onChange: (allowed?: string[], disallowed?: string[]) => void;
}

// Two modes:
// 1. Allowlist mode: Only checked tools are allowed
// 2. Blocklist mode: All tools except unchecked are allowed
```

### 7. System Prompt Presets

For future expansion, define named presets:

```typescript
const SYSTEM_PROMPT_PRESETS: Record<string, string> = {
  default: '', // Use SDK default
  concise: 'Be extremely concise. Minimize explanations.',
  verbose: 'Explain your reasoning in detail.',
  cautious: 'Always ask before making changes. Never assume.',
  autonomous: 'Work independently. Only ask when truly blocked.',
};
```

### 8. Persistence

Session settings are stored with the session in the server's session storage:

```typescript
// daemon/claude-server/src/storage/sessions.ts
interface StoredSession extends Session {
  settings: SessionSettings;
}
```

Settings persist across:
- Session resume
- Server restart (if file-based storage)
- Client reconnection

## Implementation Plan

### Phase 1: Core Settings (MVP)

1. **Server**: Add `settings` field to Session type
2. **Server**: Add `PATCH /session/:id/settings` endpoint
3. **Server**: Wire `maxTurns` to SDK options
4. **UI**: Add settings button to session header
5. **UI**: Create basic SessionSettingsModal (maxTurns only)

### Phase 2: System Prompt

1. **Server**: Wire `customSystemPrompt` and `appendSystemPrompt`
2. **UI**: Add system prompt section to modal
3. **UI**: Toggle between default/append/custom modes

### Phase 3: Tool Configuration

1. **Server**: Wire `allowedTools` and `disallowedTools`
2. **UI**: Add tool checkboxes to modal
3. **UI**: Allowlist vs blocklist mode toggle

### Phase 4: MCP Servers (Future)

1. **Server**: Wire `mcpServers` config
2. **UI**: Add MCP server configuration section
3. **UI**: Server status indicators

## Files to Create/Modify

| File | Action |
|------|--------|
| `daemon/claude-server/src/types.ts` | UPDATE: Add SessionSettings |
| `daemon/claude-server/src/routes/sessions.ts` | UPDATE: Add PATCH endpoint |
| `daemon/claude-server/src/sdk/agent.ts` | UPDATE: Wire settings to SDK |
| `daemon/claude-server/src/storage/sessions.ts` | UPDATE: Store settings |
| `app/src/features/claudecode/components/SessionSettingsModal.tsx` | NEW |
| `app/src/features/claudecode/components/SessionSettingsButton.tsx` | NEW |
| `app/src/features/claudecode/components/ToolSelector.tsx` | NEW |
| `app/src/features/claudecode/components/SystemPromptEditor.tsx` | NEW |
| `app/src/features/claudecode/hooks/useSessionSettings.ts` | NEW |
| `app/src/services/tauri.ts` | UPDATE: Add settings commands |

## API Reference

### PATCH /session/:id/settings

Update session settings.

**Request:**
```json
{
  "settings": {
    "maxTurns": 50,
    "systemPrompt": {
      "type": "append",
      "append": "Always use TypeScript."
    },
    "disallowedTools": ["WebSearch"]
  }
}
```

**Response:**
```json
{
  "id": "session-123",
  "title": "My Session",
  "settings": {
    "maxTurns": 50,
    "systemPrompt": {
      "type": "append",
      "append": "Always use TypeScript."
    },
    "disallowedTools": ["WebSearch"]
  }
}
```

### GET /session/:id

Now includes settings in response:

```json
{
  "id": "session-123",
  "title": "My Session",
  "settings": {
    "maxTurns": 100,
    "systemPrompt": { "type": "default" }
  }
}
```

## Success Criteria

- [ ] Users can set max turns per session
- [ ] Users can append custom instructions to system prompt
- [ ] Users can disable specific tools
- [ ] Settings persist across session resume
- [ ] Settings modal accessible from session header
- [ ] Default settings work without configuration

## Open Questions

1. **Workspace defaults**: Should workspaces have default settings that sessions inherit?

2. **Settings templates**: Should users be able to save/load settings presets?

3. **Per-message overrides**: Should settings be overridable per-message (like model/thinking in composer-options)?

4. **MCP scope**: When we add MCP, should it be per-session or per-workspace?

## References

- Claude SDK options: `daemon/claude-server/node_modules/@anthropic-ai/claude-code/sdk.d.ts`
- OpenCode agent config: `external/opencode/packages/opencode/src/agent/agent.ts`
- OpenCode config system: `external/opencode/packages/opencode/src/config/config.ts`
- Current session types: `daemon/claude-server/src/types.ts`
