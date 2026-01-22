# Composer Options Spec

**Status:** In Progress
**Version:** 1.0
**Last Updated:** 2026-01-22

---

## 1. Overview

### Purpose

Add model selection and extended thinking controls to the chat composer, enabling users to configure each prompt before sending.

### Goals

- Match Claude Code CLI flexibility: users can pick model and thinking mode per-message
- Provide simple preset thinking levels (not raw token sliders)
- Fetch available models dynamically from SDK

### Non-Goals

- Per-message model override (deferred to reduce complexity; model is session-level only)
- Cost estimation UI (requires cost-token-transparency spec)
- Custom thinking token values (presets only)

### References

| File | Purpose |
|------|---------|
| `daemon/claude-server/src/types.ts` | Session and request types to extend |
| `daemon/claude-server/src/sdk/agent.ts` | Wire `maxThinkingTokens` into SDK options |
| `daemon/claude-server/src/routes/messages.ts` | Accept per-message thinking override |
| `daemon/claude-server/src/routes/sessions.ts` | Session creation with new params |
| `daemon/claude-server/src/index.ts` | Register `/models` route |
| `app/src-tauri/src/lib.rs` | Tauri commands with new params |
| `app/src/services/tauri.ts` | TypeScript service signatures |
| `app/src/features/claudecode/hooks/useClaudeSession.ts` | Model/thinking state |
| `app/src/features/claudecode/components/ClaudeThreadView.tsx` | Integrate ComposerOptions |
| `daemon/claude-server/node_modules/@anthropic-ai/claude-code/sdk.d.ts` | Verify SDK types |

---

## 2. Architecture

### Components

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   UI Composer   │────▶│  Tauri Command  │────▶│  Claude Server  │
│  (dropdowns)    │     │  (prompt call)  │     │  (SDK wrapper)  │
└─────────────────┘     └─────────────────┘     └─────────────────┘
        │                                               │
        │ fetch models                                  │
        └──────────────────────────────────────────────▶│
```

### Dependencies

- `@anthropic-ai/claude-code` SDK: `supportedModels()`, `maxThinkingTokens` option
- Existing session management in `daemon/claude-server`

### Module/Folder Layout

```
daemon/claude-server/src/
├── routes/
│   ├── models.ts       # NEW: /models endpoint
│   └── messages.ts     # UPDATE: accept overrides
└── sdk/
    └── agent.ts        # UPDATE: wire thinking tokens

app/src/features/claudecode/
├── components/
│   ├── ComposerOptions.tsx        # NEW
│   ├── ModelSelector.tsx          # NEW
│   └── ThinkingModeSelector.tsx   # NEW
└── hooks/
    └── useComposerOptions.ts      # NEW
```

---

## 3. Data Model

### Core Types

#### Session (extended)

```typescript
interface Session {
  // ... existing fields
  modelId?: string;              // Already exists
  maxThinkingTokens?: number;    // NEW: thinking budget
}
```

**Semantics for `maxThinkingTokens`:**
- `undefined`: Inherit from session default (or SDK default if session has none)
- `0` or omitted in SDK call: No extended thinking
- `number > 0`: Thinking budget in tokens

#### Per-Message Override

```typescript
interface SendMessageRequest {
  parts: MessagePartInput[];
  maxThinkingTokens?: number;    // NEW: override session thinking
}
```

Note: `modelId` is session-level only (not per-message) to reduce complexity.

#### Model Info

```typescript
// SDK provides this structure
type ModelInfo = {
  value: string;       // e.g., "claude-sonnet-4-20250514"
  displayName: string; // e.g., "Claude Sonnet 4"
  description: string;
};
```

#### Thinking Modes

```typescript
type ThinkingMode = 'off' | 'low' | 'medium' | 'high' | 'max';

const THINKING_BUDGETS: Record<ThinkingMode, number | undefined> = {
  off: undefined,  // SDK default (no thinking)
  low: 4_000,
  medium: 10_000,
  high: 16_000,
  max: 32_000,
};
```

| Mode | `maxThinkingTokens` | Description |
|------|---------------------|-------------|
| `off` | `undefined` | No extended thinking |
| `low` | 4,000 | Light reasoning |
| `medium` | 10,000 | Moderate reasoning |
| `high` | 16,000 | Standard extended thinking |
| `max` | 32,000 | Maximum thinking budget |

### Storage Schema

No new storage; settings live in session state (already persisted).

---

## 4. Interfaces

### Public APIs

#### GET /models

Returns available models from SDK.

```typescript
// Response: ModelInfo[]
[
  { value: 'claude-sonnet-4-20250514', displayName: 'Claude Sonnet 4', description: 'Fast and capable' },
  { value: 'claude-opus-4-20250514', displayName: 'Claude Opus 4', description: 'Most intelligent' },
]
```

**Caching:** 5-minute TTL, per-workspace. On cache miss, call `supportedModels()`. On SDK call failure, return `FALLBACK_MODELS`.

**Fallback models:**
```typescript
const FALLBACK_MODELS: ModelInfo[] = [
  { value: 'claude-sonnet-4-20250514', displayName: 'Claude Sonnet 4', description: 'Fast and capable' },
  { value: 'claude-opus-4-20250514', displayName: 'Claude Opus 4', description: 'Most intelligent' },
  { value: 'claude-haiku-3-5-20241022', displayName: 'Claude Haiku 3.5', description: 'Fastest' },
];
```

#### POST /session

```typescript
interface CreateSessionRequest {
  title: string;
  parentId?: string;
  permission?: PermissionMode;
  modelId?: string;
  maxThinkingTokens?: number; // NEW
}
```

#### POST /session/:id/message

```typescript
interface SendMessageRequest {
  parts: MessagePartInput[];
  maxThinkingTokens?: number;  // NEW: per-message override
}
```

### Internal APIs

#### buildSdkOptions()

```typescript
// daemon/claude-server/src/sdk/agent.ts
function buildSdkOptions(
  session: Session,
  messageOverrides?: { maxThinkingTokens?: number },
  resumeId?: string,
  abortController?: AbortController
) {
  const maxThinkingTokens = messageOverrides?.maxThinkingTokens ?? session.maxThinkingTokens;

  return {
    cwd: session.directory,
    permissionMode: mapPermissionMode(session.permission),
    maxTurns: 100,
    model: session.modelId,
    maxThinkingTokens,  // SDK accepts directly
    abortController,
    resume: resumeId,
    canUseTool: createCanUseTool(session.id, session.permission),
    hooks: buildHooksConfig(session.id),
  };
}
```

### Events

SSE events should include model info for display:

```typescript
// Add to session:started or message:started event
{
  type: 'session:started',
  modelId: 'claude-sonnet-4-20250514',
  maxThinkingTokens: 16000,
  // ...existing fields
}
```

---

## 5. Workflows

### Main Flow: Model Selection

1. User opens session → UI calls `GET /models` (or uses cached)
2. Models populate dropdown with current session model selected
3. User selects different model → session update via existing session update endpoint
4. Next message uses new model

### Main Flow: Thinking Mode Selection

1. User selects thinking mode from dropdown (off/low/medium/high/max)
2. UI maps mode → `maxThinkingTokens` via `THINKING_BUDGETS`
3. On send: `maxThinkingTokens` included in message request
4. SDK receives budget and allocates thinking accordingly

### Edge Cases

#### Model Unavailable Mid-Session

If selected model becomes unavailable (API key change, deprecation):
- SDK will error on message send
- UI shows error, user can select different model
- No automatic fallback (user must explicitly choose)

#### Thinking Mode on Non-Thinking Model

All Claude models support extended thinking. If SDK rejects `maxThinkingTokens`:
- Error surfaces to user
- UI does not disable thinking selector (SDK is source of truth)

#### Controls During Streaming

Disable model/thinking dropdowns while message is streaming. Re-enable on stream complete or error.

### Retry/Backoff

N/A — uses existing message retry logic.

---

## 6. Error Handling

### Error Types

| Error | Cause | Response |
|-------|-------|----------|
| `models_fetch_failed` | `supportedModels()` SDK call fails | Return `FALLBACK_MODELS` |
| `invalid_model` | SDK rejects model ID | 400 with message |
| `invalid_thinking_budget` | SDK rejects token value | 400 with message |

### Recovery Strategy

- Model fetch: Use fallback models, log warning
- Invalid model/budget: Surface error to user, don't send message

---

## 7. Observability

### Logs

- `[models] cache hit/miss` — track model fetch frequency
- `[session] modelId={id} maxThinkingTokens={n}` — on session create/update
- `[message] thinking override: {n}` — when per-message override used

### Metrics

Deferred to future observability spec.

### Traces

N/A for MVP.

---

## 8. Security and Privacy

### AuthZ/AuthN

No additional auth required. Model availability determined by user's API key (SDK handles).

### Data Handling

Model selection stored in session state (already encrypted at rest if applicable).

---

## 9. Migration or Rollout

### Compatibility Notes

- Existing sessions without `maxThinkingTokens`: treated as `undefined` (SDK default)
- No migration needed; new fields are optional

### Rollout Plan

1. Deploy server changes (backwards compatible)
2. Deploy UI changes
3. No feature flag needed

---

## 10. Open Questions

1. ~~Model caching strategy~~ — Resolved: 5-min TTL, per-workspace

2. **Thinking token display**: Show thinking token usage separately in UI? (Requires cost-token-transparency spec)

3. ~~OpenCode parity~~ — Resolved: We expose thinking controls (OpenCode doesn't for Claude). This is a differentiator.

4. **Default model**: What model to pre-select for new sessions? Options:
   - First in `supportedModels()` list
   - Last-used model per workspace (requires persistence)
   - Hardcoded default (e.g., Sonnet 4)
