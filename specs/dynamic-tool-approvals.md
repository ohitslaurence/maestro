# Dynamic Tool Approvals

**Status:** In Progress
**Version:** 1.0
**Last Updated:** 2026-01-22

---

## 1. Overview

### Purpose

Implement interactive tool approval flow where execution pauses, the UI prompts the user, and the user's decision determines whether the tool proceeds.

### Goals

- Match Claude Code CLI behavior—when `permissionMode: 'default'`, dangerous tools ask for approval before executing
- Support "Allow Once", "Deny", and "Always Allow" responses
- Show tool-specific context (diffs, commands, file paths) in approval UI
- Handle session reconnection without losing pending permissions

### Non-Goals

- Cross-session permission persistence (approvals are session-scoped only)
- Complex glob/wildcard pattern matching for "Always Allow" (exact string matching only)
- Permission UI customization or theming

### References

| File | Purpose |
|------|---------|
| `daemon/claude-server/src/server.ts` | Current server implementation to extend with permission manager |
| `daemon/claude-server/src/sdk/permissions.ts` | Current `canUseTool` to refactor for blocking behavior |
| `daemon/claude-server/src/types.ts` | Types to extend with `PermissionRequest` |
| `daemon/claude-server/src/events/emitter.ts` | SSE emitter for permission events |
| `app/src/features/claudecode/components/ClaudeThreadView.tsx` | Thread view to integrate modal |
| `app/src/services/tauri.ts` | Tauri service layer for permission commands |
| `app/src-tauri/src/lib.rs` | Tauri command definitions |
| `external/opencode/packages/opencode/src/permission/next.ts` | Reference: blocking promise pattern |
| `external/opencode/packages/opencode/src/cli/cmd/tui/routes/session/permission.tsx` | Reference: permission UI |

---

## 2. Architecture

### Key Insight: Blocking is Possible

The Claude SDK's `canUseTool` callback returns `Promise<PermissionResult>`. This means:

```typescript
// Current (MVP): Returns immediately
return { behavior: 'allow', updatedInput: input };

// Target: Await user decision
const reply = await waitForUserReply(requestId, signal);  // Blocks!
return reply === 'allow'
  ? { behavior: 'allow', updatedInput: input }
  : { behavior: 'deny', message: 'User denied' };
```

The SDK execution loop pauses until the Promise resolves.

### Components

| Component | Current Status | Target Behavior |
|-----------|----------------|-----------------|
| Server `canUseTool` | Auto-approves all | Blocks on dangerous tools |
| PermissionManager | Missing | Manages pending requests, approvals |
| Permission Reply Endpoint | Missing | Receives UI decisions |
| SSE Events | Partial | Emits full `PermissionRequest` |
| UI Modal | None | Shows context, captures decision |

### Dependencies

- `@anthropic-ai/claude-code` SDK (provides `CanUseTool` callback type)
- Hono (HTTP routing)
- Existing SSE event infrastructure

---

## 3. Data Model

### Permission Request

```typescript
interface PermissionRequest {
  id: string;                          // UUID
  sessionId: string;
  messageId: string;                   // Current assistant message ID
  tool: string;                        // Tool name (Read, Write, Bash, etc.)
  permission: string;                  // Permission type (read, write, bash, etc.)
  input: Record<string, unknown>;      // Tool input (filepath, command, etc.)
  patterns: string[];                  // Affected patterns (exact strings, not globs)
  metadata: PermissionMetadata;        // Tool-specific context
  suggestions: PermissionSuggestion[]; // SDK-provided "always allow" patterns
  createdAt: number;
}

interface PermissionMetadata {
  description?: string;                // Human-readable description
  filePath?: string;                   // File operations
  diff?: string;                       // Edit: unified diff
  command?: string;                    // Bash command
  url?: string;                        // WebFetch URL
  query?: string;                      // WebSearch query
}

interface PermissionSuggestion {
  type: 'addRules' | 'addDirectories';
  patterns: string[];
  description: string;
}
```

### Permission Reply

```typescript
interface PermissionReplyRequest {
  reply: 'allow' | 'deny' | 'always';
  message?: string;                    // Feedback on deny
}

interface PermissionReplyResponse {
  success: boolean;
  error?: string;
}
```

### Session Permission State

```typescript
interface PendingPermission {
  request: PermissionRequest;
  resolve: (result: PermissionResult) => void;
  reject: (error: Error) => void;
  signal: AbortSignal;
  timeoutId: ReturnType<typeof setTimeout>;
}

// Server maintains per-session:
// Key: sessionId → Map<requestId, PendingPermission>
const pendingPermissions = new Map<string, Map<string, PendingPermission>>();

// Approved patterns for session (from "always" replies)
// Key: sessionId → Set<pattern> (exact string matching)
const sessionApprovals = new Map<string, Set<string>>();

// Reverse lookup for O(1) session finding
// Key: requestId → sessionId
const requestToSession = new Map<string, string>();
```

---

## 4. Interfaces

### Permission Manager

```typescript
// daemon/claude-server/src/permissions/manager.ts

export class PermissionManager {
  private pending = new Map<string, Map<string, PendingPermission>>();
  private approved = new Map<string, Set<string>>();
  private requestToSession = new Map<string, string>();

  /**
   * Request permission for a tool invocation.
   * Returns a Promise that blocks until user replies or timeout.
   */
  async request(
    sessionId: string,
    messageId: string,
    request: Omit<PermissionRequest, 'id' | 'createdAt' | 'sessionId' | 'messageId'>,
    signal: AbortSignal
  ): Promise<PermissionResult> {
    const id = crypto.randomUUID();
    const fullRequest: PermissionRequest = {
      ...request,
      id,
      sessionId,
      messageId,
      createdAt: Date.now(),
    };

    // Check if already approved by "always" pattern (exact match)
    if (this.isApproved(sessionId, request.patterns)) {
      return { behavior: 'allow', updatedInput: request.input };
    }

    // Create blocking Promise
    return new Promise((resolve, reject) => {
      const timeoutId = setTimeout(() => {
        this.reply(sessionId, id, 'deny', 'Permission request timed out');
      }, PERMISSION_TIMEOUT_MS);

      // Store pending request
      if (!this.pending.has(sessionId)) {
        this.pending.set(sessionId, new Map());
      }
      this.pending.get(sessionId)!.set(id, {
        request: fullRequest,
        resolve,
        reject,
        signal,
        timeoutId,
      });
      this.requestToSession.set(id, sessionId);

      // Emit SSE event
      sseEmitter.emit('permission.asked', { request: fullRequest });

      // Handle abort
      signal.addEventListener('abort', () => {
        clearTimeout(timeoutId);
        this.pending.get(sessionId)?.delete(id);
        this.requestToSession.delete(id);
        reject(new Error('Aborted'));
      });
    });
  }

  /**
   * Reply to a pending permission request.
   */
  reply(
    sessionId: string,
    requestId: string,
    reply: 'allow' | 'deny' | 'always',
    message?: string
  ): boolean {
    const sessionPending = this.pending.get(sessionId);
    const pending = sessionPending?.get(requestId);
    if (!pending) return false;

    clearTimeout(pending.timeoutId);

    if (reply === 'deny') {
      pending.resolve({
        behavior: 'deny',
        message: message || 'User denied permission',
      });
    } else {
      if (reply === 'always') {
        this.addApprovals(sessionId, pending.request.patterns);
      }
      pending.resolve({
        behavior: 'allow',
        updatedInput: pending.request.input,
      });
    }

    sessionPending.delete(requestId);
    this.requestToSession.delete(requestId);
    sseEmitter.emit('permission.replied', { sessionId, requestId, reply });

    return true;
  }

  /**
   * Get pending permissions for a session (for reconnection).
   */
  getPending(sessionId?: string): PermissionRequest[] {
    if (sessionId) {
      const sessionPending = this.pending.get(sessionId);
      return sessionPending
        ? Array.from(sessionPending.values()).map(p => p.request)
        : [];
    }
    // Return all pending (admin use)
    const all: PermissionRequest[] = [];
    for (const sessionMap of this.pending.values()) {
      for (const p of sessionMap.values()) {
        all.push(p.request);
      }
    }
    return all;
  }

  /**
   * Find session ID for a request ID (O(1) lookup).
   */
  findSessionForRequest(requestId: string): string | undefined {
    return this.requestToSession.get(requestId);
  }

  /**
   * Check if patterns are approved (exact string match).
   */
  private isApproved(sessionId: string, patterns: string[]): boolean {
    const approved = this.approved.get(sessionId);
    if (!approved) return false;
    return patterns.every(p => approved.has(p));
  }

  private addApprovals(sessionId: string, patterns: string[]): void {
    if (!this.approved.has(sessionId)) {
      this.approved.set(sessionId, new Set());
    }
    patterns.forEach(p => this.approved.get(sessionId)!.add(p));
  }

  clearSession(sessionId: string): void {
    const sessionPending = this.pending.get(sessionId);
    if (sessionPending) {
      for (const pending of sessionPending.values()) {
        clearTimeout(pending.timeoutId);
        pending.reject(new Error('Session cleared'));
      }
      for (const requestId of sessionPending.keys()) {
        this.requestToSession.delete(requestId);
      }
    }
    this.pending.delete(sessionId);
    this.approved.delete(sessionId);
  }
}

const PERMISSION_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes
export const permissionManager = new PermissionManager();
```

### canUseTool Handler

```typescript
// daemon/claude-server/src/sdk/permissions.ts

export function createCanUseTool(
  sessionId: string,
  permissionMode: PermissionMode,
  getMessageId: () => string  // Callback to get current message ID
): CanUseTool {
  return async (
    toolName: string,
    input: Record<string, unknown>,
    options: { signal: AbortSignal; suggestions?: PermissionUpdate[] }
  ): Promise<PermissionResult> => {
    // Bypass mode: auto-approve everything
    if (permissionMode === 'bypassPermissions') {
      return { behavior: 'allow', updatedInput: input };
    }

    // Accept edits mode: auto-approve file operations
    if (permissionMode === 'acceptEdits') {
      const fileTools = ['Read', 'Write', 'Edit', 'Glob', 'Grep'];
      if (fileTools.includes(toolName)) {
        return { behavior: 'allow', updatedInput: input };
      }
    }

    // Default mode: ask for dangerous tools
    const dangerousTools = ['Write', 'Edit', 'Bash', 'WebFetch', 'WebSearch'];
    if (!dangerousTools.includes(toolName)) {
      return { behavior: 'allow', updatedInput: input };
    }

    // Build permission request
    const request = buildPermissionRequest(toolName, input, options.suggestions);

    // Block until user replies
    return permissionManager.request(
      sessionId,
      getMessageId(),
      request,
      options.signal
    );
  };
}
```

### HTTP Endpoints

```typescript
// daemon/claude-server/src/routes/permissions.ts

import { Hono } from 'hono';
import { permissionManager } from '../permissions/manager';

const app = new Hono();

/**
 * POST /permission/:requestId/reply
 * Reply to a pending permission request.
 */
app.post('/:requestId/reply', async (c) => {
  const { requestId } = c.req.param();
  const body = await c.req.json<{
    reply: 'allow' | 'deny' | 'always';
    message?: string;
  }>();

  // O(1) session lookup
  const sessionId = permissionManager.findSessionForRequest(requestId);
  if (!sessionId) {
    return c.json({ success: false, error: 'Request not found' }, 404);
  }

  const success = permissionManager.reply(
    sessionId,
    requestId,
    body.reply,
    body.message
  );

  if (!success) {
    return c.json({ success: false, error: 'Request expired or not found' }, 404);
  }

  return c.json({ success: true });
});

/**
 * GET /permission/pending
 * List pending permission requests (for reconnecting clients).
 */
app.get('/pending', (c) => {
  const sessionId = c.req.query('sessionId');
  const pending = permissionManager.getPending(sessionId);
  return c.json({ requests: pending });
});

export default app;
```

### SSE Events

```typescript
// permission.asked event
{
  type: 'permission.asked',
  properties: {
    request: PermissionRequest;  // Full request object with metadata
  }
}

// permission.replied event
{
  type: 'permission.replied',
  properties: {
    sessionId: string;
    requestId: string;
    reply: 'allow' | 'deny' | 'always';
  }
}
```

---

## 5. Workflows

### Main Flow: Tool Permission Request

```
1. SDK calls canUseTool(toolName, input, options)
2. createCanUseTool checks permissionMode
3. If dangerous tool in default mode:
   a. permissionManager.request() creates pending Promise
   b. SSE emits 'permission.asked' with full PermissionRequest
   c. UI shows PermissionModal
   d. User clicks Allow/Deny/Always
   e. UI calls POST /permission/:id/reply
   f. permissionManager.reply() resolves Promise
   g. SDK continues with allow/deny result
```

### Concurrent Tool Handling

When the agent invokes multiple tools in parallel:

1. Each tool call creates its own pending permission
2. UI maintains a queue of pending requests
3. Modal shows first queued request
4. On reply, modal shifts to next queued request
5. All pending permissions block independently

```typescript
// UI state for concurrent permissions
const [pendingQueue, setPendingQueue] = useState<PermissionRequest[]>([]);
const currentRequest = pendingQueue[0] ?? null;

// On permission.asked, append to queue
// On reply, shift queue
```

### Client Reconnection

```
1. Client establishes SSE connection
2. Client calls GET /permission/pending?sessionId=...
3. If pending requests exist, populate UI queue
4. User can reply to any pending request
```

### Abort Handling

```
1. User aborts session (stop button)
2. AbortSignal fires on all pending permissions
3. Each pending Promise rejects with 'Aborted'
4. Timeouts are cleared
5. Session state is cleaned up
```

### Denial Memory

When a user denies a permission, the denial is **not** remembered. If the agent attempts the same operation again, the user will be prompted again. This is intentional—the user may have denied due to timing rather than the operation itself.

---

## 6. Error Handling

### Error Types

| Error | Trigger | Recovery |
|-------|---------|----------|
| `PermissionTimeout` | No reply within 5 minutes | Auto-deny with message |
| `SessionAborted` | User stops session | Reject all pending |
| `RequestNotFound` | Reply to expired/invalid ID | Return 404, UI clears modal |
| `NetworkError` | Reply endpoint unreachable | UI shows retry option |

### Recovery Strategy

- **Timeout**: Treat as deny; agent can retry if appropriate
- **Abort**: Clean termination; no recovery needed
- **Network error**: UI should retry with exponential backoff (3 attempts)
- **Invalid request**: Clear UI state; log for debugging

---

## 7. Observability

### Logs

| Event | Level | Data |
|-------|-------|------|
| Permission requested | INFO | sessionId, tool, patterns |
| Permission replied | INFO | sessionId, requestId, reply |
| Permission timeout | WARN | sessionId, requestId |
| Session cleanup | DEBUG | sessionId, pending count |

### Metrics (Future)

- `permission_requests_total` (counter, labels: tool, outcome)
- `permission_response_time_ms` (histogram)
- `permission_timeout_total` (counter)

---

## 8. Security and Privacy

### Session Isolation

- Permissions are strictly session-scoped
- One session cannot reply to another session's permissions
- `requestToSession` map enforces this at lookup time

### Input Validation

- Reply endpoint validates `reply` is one of 'allow' | 'deny' | 'always'
- Request ID is validated as existing before processing
- Tool input is passed through unchanged (SDK responsibility)

### No Credential Exposure

- Permission requests may contain file paths and commands
- These are displayed to the local user only
- No permission data is sent to external services

---

## 9. Migration and Rollout

### Compatibility with MVP

The current MVP auto-approves all tools. Migration:

1. Deploy PermissionManager with feature flag
2. Default to `permissionMode: 'bypassPermissions'` initially
3. Enable `permissionMode: 'default'` per-session via config
4. UI gracefully handles missing permission events (no-op)

### Rollout Plan

1. **Phase 1**: Server infrastructure (no UI change, feature disabled)
2. **Phase 2**: UI components (modal exists but never shown)
3. **Phase 3**: Enable for new sessions via config flag
4. **Phase 4**: Default to enabled for all sessions

---

## 10. Open Questions

1. **Glob pattern support**: Should "Always Allow" support wildcards (e.g., `/src/**`)? Current spec uses exact string matching only. Could add minimatch in future.

2. **Cross-session persistence**: Should approved patterns persist to disk for reuse across sessions? Deferred to future spec.

3. **Subagent inheritance**: If `Task` tool spawns a subagent, should it inherit parent session's approvals? Currently no—each session is independent.

4. **Structured deny reasons**: Should deny have categories (security, wrong file, different approach)? Current spec uses optional freeform message.

---

## UI Components

### PermissionModal

```tsx
interface PermissionModalProps {
  request: PermissionRequest | null;
  onReply: (reply: 'allow' | 'deny' | 'always', message?: string) => void;
  onClose: () => void;
}

function PermissionModal({ request, onReply, onClose }: PermissionModalProps) {
  if (!request) return null;

  return (
    <Modal open onClose={onClose}>
      <ModalHeader>
        <ToolIcon name={request.tool} />
        <span>Permission Required</span>
      </ModalHeader>

      <ModalBody>
        <PermissionContext request={request} />
      </ModalBody>

      <ModalFooter>
        <Button variant="danger" onClick={() => onReply('deny')}>
          Deny
        </Button>
        <Button variant="secondary" onClick={() => onReply('always')}>
          Always Allow
        </Button>
        <Button variant="primary" onClick={() => onReply('allow')}>
          Allow Once
        </Button>
      </ModalFooter>
    </Modal>
  );
}
```

### PermissionContext

```tsx
function PermissionContext({ request }: { request: PermissionRequest }) {
  switch (request.tool) {
    case 'Edit':
      return (
        <div>
          <p>Edit file: <code>{request.metadata.filePath}</code></p>
          <DiffViewer diff={request.metadata.diff} />
        </div>
      );

    case 'Bash':
      return (
        <div>
          <p>Run command:</p>
          <CodeBlock language="bash">{request.metadata.command}</CodeBlock>
        </div>
      );

    case 'Write':
      return (
        <div>
          <p>Write file: <code>{request.metadata.filePath}</code></p>
        </div>
      );

    default:
      return (
        <div>
          <p>{request.metadata.description}</p>
          <pre>{JSON.stringify(request.input, null, 2)}</pre>
        </div>
      );
  }
}
```

### usePermissions Hook

```typescript
interface UsePermissionsReturn {
  pendingQueue: PermissionRequest[];
  currentRequest: PermissionRequest | null;
  reply: (reply: 'allow' | 'deny' | 'always', message?: string) => Promise<void>;
  dismiss: () => void;
}

function usePermissions(sessionId: string | null): UsePermissionsReturn {
  const [pendingQueue, setPendingQueue] = useState<PermissionRequest[]>([]);

  // Subscribe to permission.asked events
  useEffect(() => {
    if (!sessionId) return;

    // Fetch any existing pending on mount/reconnect
    fetch(`/permission/pending?sessionId=${sessionId}`)
      .then(r => r.json())
      .then(data => setPendingQueue(data.requests));

    const unsubscribe = subscribeToAgentEvents((event) => {
      if (event.type === 'permission.asked') {
        setPendingQueue(q => [...q, event.properties.request]);
      }
      if (event.type === 'permission.replied') {
        setPendingQueue(q => q.filter(r => r.id !== event.properties.requestId));
      }
    });

    return unsubscribe;
  }, [sessionId]);

  const reply = useCallback(async (
    replyType: 'allow' | 'deny' | 'always',
    message?: string
  ) => {
    const current = pendingQueue[0];
    if (!current) return;

    await fetch(`/permission/${current.id}/reply`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ reply: replyType, message }),
    });
    // SSE event will remove from queue
  }, [pendingQueue]);

  const dismiss = useCallback(() => {
    if (pendingQueue.length > 0) {
      reply('deny', 'Dismissed');
    }
  }, [pendingQueue, reply]);

  return {
    pendingQueue,
    currentRequest: pendingQueue[0] ?? null,
    reply,
    dismiss,
  };
}
```

---

## Files to Create/Modify

| File | Action |
|------|--------|
| `daemon/claude-server/src/permissions/manager.ts` | CREATE |
| `daemon/claude-server/src/permissions/types.ts` | CREATE |
| `daemon/claude-server/src/routes/permissions.ts` | CREATE |
| `daemon/claude-server/src/sdk/permissions.ts` | MODIFY (blocking) |
| `daemon/claude-server/src/types.ts` | MODIFY (PermissionRequest) |
| `daemon/claude-server/src/index.ts` | MODIFY (mount routes) |
| `app/src/features/claudecode/components/PermissionModal.tsx` | CREATE |
| `app/src/features/claudecode/components/PermissionContext.tsx` | CREATE |
| `app/src/features/claudecode/hooks/usePermissions.ts` | CREATE |
| `app/src/features/claudecode/components/ClaudeThreadView.tsx` | MODIFY |
| `app/src/services/tauri.ts` | MODIFY (permission reply) |
