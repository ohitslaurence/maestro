# Composer Options Spec

## Overview

Add model selection and extended thinking controls to the chat composer, enabling users to configure each prompt before sending.

**Goal**: Match Claude Code CLI flexibility—users can pick model and thinking mode per-message, not just per-session.

## Current State

| Layer | Model Selection | Thinking Budget |
|-------|-----------------|-----------------|
| **Server** | `session.modelId` passed to SDK | Not wired |
| **UI** | No selector | No UI |
| **Hook** | Hardcoded in `create()` | N/A |

## SDK Capabilities (Verified)

The `@anthropic-ai/claude-code` SDK exposes (from `sdk.d.ts`):

```typescript
export type Options = {
  model?: string;
  maxThinkingTokens?: number;  // ✅ Thinking budget supported!
  // ...
};

// Runtime control methods on Query:
setModel(model?: string): Promise<void>;
supportedModels(): Promise<ModelInfo[]>;  // ✅ Dynamic model list!
```

**Key findings:**
- `maxThinkingTokens` directly controls thinking budget
- `supportedModels()` returns available models dynamically
- `setModel()` allows changing model mid-session

## OpenCode Reference

OpenCode uses **variants** that map to provider-specific options:

```typescript
// external/opencode/packages/opencode/src/provider/transform.ts
high: { thinking: { type: 'enabled', budgetTokens: 16000 } },
max:  { thinking: { type: 'enabled', budgetTokens: 31999 } },
```

We'll adopt a similar UX pattern (preset levels, not raw token slider).

## Design

### 1. Data Model

#### Session (extended)

```typescript
interface Session {
  // ... existing fields
  modelId?: string;              // Already exists
  maxThinkingTokens?: number;    // NEW: 0 = off, or token budget
}
```

#### Per-Message Override

```typescript
interface SendMessageRequest {
  parts: MessagePartInput[];
  modelId?: string;              // NEW: override session model
  maxThinkingTokens?: number;    // NEW: override session thinking
}
```

### 2. Available Models

Fetch dynamically via `supportedModels()` on session init:

```typescript
// SDK provides this structure
type ModelInfo = {
  value: string;       // e.g., "claude-sonnet-4-20250514"
  displayName: string; // e.g., "Claude Sonnet 4"
  description: string;
};

// Fallback if fetch fails
const FALLBACK_MODELS: ModelInfo[] = [
  { value: 'claude-sonnet-4-20250514', displayName: 'Claude Sonnet 4', description: 'Fast and capable' },
  { value: 'claude-opus-4-20250514', displayName: 'Claude Opus 4', description: 'Most intelligent' },
  { value: 'claude-haiku-3-5-20241022', displayName: 'Claude Haiku 3.5', description: 'Fastest' },
];
```

### 3. Thinking Modes

Map user-friendly levels to token budgets:

| Mode | `maxThinkingTokens` | Description |
|------|---------------------|-------------|
| `off` | `undefined` (omit) | No extended thinking |
| `low` | 4,000 | Light reasoning |
| `medium` | 10,000 | Moderate reasoning |
| `high` | 16,000 | Standard extended thinking |
| `max` | 32,000 | Maximum thinking budget |

```typescript
type ThinkingMode = 'off' | 'low' | 'medium' | 'high' | 'max';

const THINKING_BUDGETS: Record<ThinkingMode, number | undefined> = {
  off: undefined,
  low: 4_000,
  medium: 10_000,
  high: 16_000,
  max: 32_000,
};
```

### 4. UI Components

#### ComposerOptions (new component)

Compact options bar above the textarea:

```
┌──────────────────────────────────────────────────────────┐
│ [Sonnet 4 ▾]  [💭 High ▾]                               │
├──────────────────────────────────────────────────────────┤
│                                                          │
│  Type your message...                                    │
│                                                          │
│                                            [Send ↵]      │
└──────────────────────────────────────────────────────────┘
```

- **Model dropdown**: Shows current model, click to change
- **Thinking dropdown**: Shows thinking mode (off/low/medium/high/max)

#### Model Selector Dropdown

```tsx
interface ModelSelectorProps {
  value: string;
  onChange: (modelId: string) => void;
  models: ModelInfo[];
  loading?: boolean;
}

// Renders as compact dropdown
// Shows model displayName
// Fetches models via supportedModels() on mount
```

#### Thinking Mode Selector

```tsx
interface ThinkingModeSelectorProps {
  value: ThinkingMode;
  onChange: (mode: ThinkingMode) => void;
}

// Renders as dropdown with options:
// Off, Low (4k), Medium (10k), High (16k), Max (32k)
```

### 5. Server Changes

#### Session Create (update)

```typescript
// POST /session
interface CreateSessionRequest {
  title: string;
  parentId?: string;
  permission?: PermissionMode;
  modelId?: string;
  maxThinkingTokens?: number; // NEW
}
```

#### Message Send (update)

```typescript
// POST /session/:id/message
interface SendMessageRequest {
  parts: MessagePartInput[];
  modelId?: string;            // NEW: per-message override
  maxThinkingTokens?: number;  // NEW: per-message override
}
```

#### SDK Options Builder (update)

```typescript
// daemon/claude-server/src/sdk/agent.ts
function buildSdkOptions(
  session: Session,
  messageOverrides?: { modelId?: string; maxThinkingTokens?: number },
  resumeId?: string,
  abortController?: AbortController
) {
  const modelId = messageOverrides?.modelId ?? session.modelId;
  const maxThinkingTokens = messageOverrides?.maxThinkingTokens ?? session.maxThinkingTokens;

  return {
    cwd: session.directory,
    permissionMode: mapPermissionMode(session.permission),
    maxTurns: 100,
    model: modelId,
    maxThinkingTokens,  // SDK accepts this directly
    abortController,
    resume: resumeId,
    canUseTool: createCanUseTool(session.id, session.permission),
    hooks: buildHooksConfig(session.id),
  };
}
```

#### Models Endpoint (new)

```typescript
// GET /models
// Returns available models from SDK
app.get('/models', async (c) => {
  // Call supportedModels() on active query or cache from init
  const models = await getSupportedModels();
  return c.json(models);
});
```

### 6. Hook Changes

#### useClaudeSession (update)

```typescript
export type ClaudeSessionState = {
  // ... existing

  // Model & thinking
  modelId: string;
  setModelId: (id: string) => void;
  thinkingMode: ThinkingMode;
  setThinkingMode: (mode: ThinkingMode) => void;

  // Available models (fetched from SDK)
  models: ModelInfo[];
  modelsLoading: boolean;

  // Updated prompt signature
  prompt: (message: string, options?: {
    modelId?: string;
    maxThinkingTokens?: number;
  }) => Promise<void>;
};
```

#### useComposerOptions (new hook)

```typescript
interface UseComposerOptionsReturn {
  // Current values (session defaults)
  modelId: string;
  thinkingMode: ThinkingMode;

  // Handlers
  setModelId: (id: string) => void;
  setThinkingMode: (mode: ThinkingMode) => void;

  // Available options
  models: ModelInfo[];
  modelsLoading: boolean;

  // Derived
  maxThinkingTokens: number | undefined; // Computed from thinkingMode
}
```

### 7. Tauri Commands (update)

```rust
#[tauri::command]
async fn claude_sdk_session_prompt(
    state: State<'_, AppState>,
    workspace_id: String,
    session_id: String,
    message: String,
    model_id: Option<String>,           // NEW
    max_thinking_tokens: Option<u32>,   // NEW
) -> Result<(), String>

#[tauri::command]
async fn claude_sdk_models(
    state: State<'_, AppState>,
    workspace_id: String,
) -> Result<Vec<ModelInfo>, String>  // NEW: fetch available models
```

### 8. Service Layer (update)

```typescript
// app/src/services/tauri.ts
export async function claudeSdkSessionPrompt(
  workspaceId: string,
  sessionId: string,
  message: string,
  options?: {
    modelId?: string;
    maxThinkingTokens?: number;
  }
): Promise<void> {
  return invoke('claude_sdk_session_prompt', {
    workspaceId,
    sessionId,
    message,
    modelId: options?.modelId,
    maxThinkingTokens: options?.maxThinkingTokens,
  });
}

export async function claudeSdkModels(
  workspaceId: string
): Promise<ModelInfo[]> {
  return invoke('claude_sdk_models', { workspaceId });
}
```

## Implementation Plan

### Phase 1: Model Selection (MVP)

1. **Server**: Add `/models` endpoint, accept `modelId` in message request
2. **UI**: Add model dropdown to composer (fetch models on mount)
3. **Hook**: Wire model selection to prompt call
4. **Tauri**: Add `model_id` param + new models command

### Phase 2: Thinking Mode

1. **Server**: Wire `maxThinkingTokens` to SDK options
2. **UI**: Add thinking mode dropdown to composer
3. **Hook**: Map ThinkingMode → maxThinkingTokens

### Phase 3: Persistence & Defaults

1. **Session defaults**: Store last-used model/thinking per session
2. **Workspace defaults**: Store preferred model per workspace

## Files to Modify

| File | Changes |
|------|---------|
| `daemon/claude-server/src/types.ts` | Update request types |
| `daemon/claude-server/src/sdk/agent.ts` | Wire `maxThinkingTokens` |
| `daemon/claude-server/src/routes/messages.ts` | Accept message overrides |
| `daemon/claude-server/src/routes/models.ts` | NEW: models endpoint |
| `app/src-tauri/src/lib.rs` | Add params + new command |
| `app/src/services/tauri.ts` | Update service signatures |
| `app/src/features/claudecode/hooks/useClaudeSession.ts` | Add model/thinking state |
| `app/src/features/claudecode/components/ClaudeThreadView.tsx` | Add ComposerOptions |
| `app/src/features/claudecode/components/ComposerOptions.tsx` | NEW |
| `app/src/features/claudecode/components/ModelSelector.tsx` | NEW |
| `app/src/features/claudecode/components/ThinkingModeSelector.tsx` | NEW |

## Open Questions

1. **Model caching**: Cache `supportedModels()` result? How long? Per-session or global?

2. **Thinking token display**: Should we show thinking token usage separately in the UI? (Prerequisite: spec #2 cost-token-transparency)

3. **OpenCode parity**: OpenCode doesn't expose thinking for Claude (only model variants). Do we want parity, or leverage SDK capabilities?

## Success Criteria

- [ ] User can select model from dropdown before sending message
- [ ] Models are fetched dynamically from SDK
- [ ] User can select thinking mode (off/low/medium/high/max)
- [ ] Model + thinking settings persist for session
- [ ] Per-message overrides work correctly
- [ ] UI updates to show current model/thinking state

## References

- **Claude SDK types**: `daemon/claude-server/node_modules/@anthropic-ai/claude-code/sdk.d.ts`
- OpenCode model handling: `external/opencode/packages/opencode/src/provider/`
- OpenCode transform (thinking): `external/opencode/packages/opencode/src/provider/transform.ts`
- Current server types: `daemon/claude-server/src/types.ts`
- Current SDK wrapper: `daemon/claude-server/src/sdk/agent.ts`
