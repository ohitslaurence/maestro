# Dynamic Tool Approvals Spec

## Overview

Implement interactive tool approval flow where execution pauses, the UI prompts the user, and the user's decision determines whether the tool proceeds.

**Goal**: Match Claude Code CLI behavior—when `permissionMode: 'default'`, dangerous tools ask for approval before executing.

## Current State

| Component | Status | Behavior |
|-----------|--------|----------|
| **Server `canUseTool`** | MVP | Auto-approves all tools immediately |
| **SSE Events** | Partial | Emits `permission.asked`/`permission.replied` but non-blocking |
| **Permission Reply Endpoint** | Missing | No way for UI to send reply |
| **UI** | None | No approval modal/prompt |

## Key Insight: Blocking is Possible

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

## OpenCode Reference

OpenCode uses blocking Promises with event-driven resolution:

```typescript
// external/opencode/packages/opencode/src/permission/next.ts
export const ask = async (input: {
  permission: string;
  patterns: string[];
  sessionID: string;
  metadata: Record<string, any>;
  always: string[];
}): Promise<void> => {
  // 1. Check if already approved by ruleset
  const rule = evaluate(input.permission, input.patterns, ...rulesets);
  if (rule.action === 'allow') return;
  if (rule.action === 'deny') throw new DeniedError();

  // 2. Create pending request with unresolved Promise
  const request = createPendingRequest(input);

  // 3. Emit event for UI
  Event.publish('permission.asked', request);

  // 4. Block until reply
  return request.promise;  // Awaited by tool
};

export const reply = (id: string, reply: 'once' | 'always' | 'reject') => {
  const pending = pendingRequests.get(id);
  if (!pending) return;

  if (reply === 'reject') {
    pending.reject(new RejectedError());
  } else {
    if (reply === 'always') {
      addToApprovedRuleset(pending.patterns);
    }
    pending.resolve();
  }
  pendingRequests.delete(id);
};
```

## Design

### 1. Data Model

#### Permission Request

```typescript
interface PermissionRequest {
  id: string;                          // UUID
  sessionId: string;
  messageId: string;                   // Current assistant message
  tool: string;                        // Tool name (Read, Write, Bash, etc.)
  permission: string;                  // Permission type (read, write, bash, etc.)
  input: Record<string, unknown>;      // Tool input (filepath, command, etc.)
  patterns: string[];                  // Affected patterns (file paths, globs)
  metadata: PermissionMetadata;        // Tool-specific context
  suggestions: PermissionSuggestion[]; // SDK-provided "always allow" patterns
  createdAt: number;
}

interface PermissionMetadata {
  // Common
  description?: string;                // Human-readable description

  // File operations (Read, Write, Edit, Glob, Grep)
  filePath?: string;
  diff?: string;                       // For Edit: unified diff

  // Bash
  command?: string;

  // WebFetch/WebSearch
  url?: string;
  query?: string;
}

interface PermissionSuggestion {
  type: 'addRules' | 'addDirectories';
  patterns: string[];
  description: string;
}
```

#### Permission Reply

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

#### Session Permission State

```typescript
interface PendingPermission {
  request: PermissionRequest;
  resolve: (result: PermissionResult) => void;
  reject: (error: Error) => void;
  signal: AbortSignal;
}

// Server maintains per-session:
const pendingPermissions = new Map<string, Map<string, PendingPermission>>();
// Key: sessionId → Map<requestId, PendingPermission>

// Approved patterns for session (from "always" replies)
const sessionApprovals = new Map<string, Set<string>>();
// Key: sessionId → Set<pattern>
```

### 2. Server Changes

#### Permission Module (new)

```typescript
// daemon/claude-server/src/permissions/manager.ts

export class PermissionManager {
  private pending = new Map<string, Map<string, PendingPermission>>();
  private approved = new Map<string, Set<string>>();

  /**
   * Request permission for a tool invocation.
   * Returns a Promise that blocks until user replies.
   */
  async request(
    sessionId: string,
    request: Omit<PermissionRequest, 'id' | 'createdAt'>,
    signal: AbortSignal
  ): Promise<PermissionResult> {
    const id = crypto.randomUUID();
    const fullRequest: PermissionRequest = {
      ...request,
      id,
      createdAt: Date.now(),
    };

    // Check if already approved by "always" pattern
    if (this.isApproved(sessionId, request.patterns)) {
      return { behavior: 'allow', updatedInput: request.input };
    }

    // Create blocking Promise
    return new Promise((resolve, reject) => {
      // Store pending request
      if (!this.pending.has(sessionId)) {
        this.pending.set(sessionId, new Map());
      }
      this.pending.get(sessionId)!.set(id, {
        request: fullRequest,
        resolve,
        reject,
        signal,
      });

      // Emit SSE event
      sseEmitter.emit('permission.asked', { request: fullRequest });

      // Handle abort
      signal.addEventListener('abort', () => {
        this.pending.get(sessionId)?.delete(id);
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
    sseEmitter.emit('permission.replied', {
      sessionId,
      requestId,
      reply,
    });

    return true;
  }

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
    this.pending.delete(sessionId);
    this.approved.delete(sessionId);
  }
}

export const permissionManager = new PermissionManager();
```

#### Update canUseTool (blocking)

```typescript
// daemon/claude-server/src/sdk/permissions.ts

export function createCanUseTool(
  sessionId: string,
  permissionMode: PermissionMode
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
    const request = buildPermissionRequest(sessionId, toolName, input, options.suggestions);

    // Block until user replies
    return permissionManager.request(sessionId, request, options.signal);
  };
}

function buildPermissionRequest(
  sessionId: string,
  toolName: string,
  input: Record<string, unknown>,
  suggestions?: PermissionUpdate[]
): Omit<PermissionRequest, 'id' | 'createdAt'> {
  const patterns = extractPatterns(toolName, input);
  const metadata = extractMetadata(toolName, input);

  return {
    sessionId,
    messageId: '', // Filled by caller
    tool: toolName,
    permission: toolName.toLowerCase(),
    input,
    patterns,
    metadata,
    suggestions: (suggestions || []).map(s => ({
      type: s.type,
      patterns: s.directories || [],
      description: `Allow ${s.type} for these paths`,
    })),
  };
}

function extractPatterns(toolName: string, input: Record<string, unknown>): string[] {
  switch (toolName) {
    case 'Read':
    case 'Write':
    case 'Edit':
      return [input.file_path as string].filter(Boolean);
    case 'Glob':
      return [input.pattern as string].filter(Boolean);
    case 'Grep':
      return [input.path as string, input.pattern as string].filter(Boolean);
    case 'Bash':
      return [input.command as string].filter(Boolean);
    case 'WebFetch':
      return [input.url as string].filter(Boolean);
    case 'WebSearch':
      return [input.query as string].filter(Boolean);
    default:
      return [];
  }
}

function extractMetadata(toolName: string, input: Record<string, unknown>): PermissionMetadata {
  switch (toolName) {
    case 'Read':
      return { filePath: input.file_path as string, description: `Read file` };
    case 'Write':
      return { filePath: input.file_path as string, description: `Write file` };
    case 'Edit':
      return {
        filePath: input.file_path as string,
        diff: `- ${input.old_string}\n+ ${input.new_string}`,
        description: `Edit file`,
      };
    case 'Bash':
      return { command: input.command as string, description: `Run command` };
    case 'WebFetch':
      return { url: input.url as string, description: `Fetch URL` };
    case 'WebSearch':
      return { query: input.query as string, description: `Web search` };
    default:
      return { description: `Use ${toolName}` };
  }
}
```

#### Permission Reply Endpoint (new)

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

  // Find session for this request (search all sessions)
  const sessionId = findSessionForRequest(requestId);
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
 * List all pending permission requests (for reconnecting clients).
 */
app.get('/pending', (c) => {
  const sessionId = c.req.query('sessionId');
  const pending = permissionManager.getPending(sessionId);
  return c.json({ requests: pending });
});

export default app;
```

### 3. SSE Event Updates

#### Enhanced permission.asked Event

```typescript
// Before (MVP)
{
  type: 'permission.asked',
  properties: {
    id: string;
    sessionId: string;
    permission: 'tool_use';
    tool?: string;
  }
}

// After
{
  type: 'permission.asked',
  properties: {
    request: PermissionRequest;  // Full request object with metadata
  }
}
```

### 4. UI Components

#### PermissionModal

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

#### PermissionContext (tool-specific rendering)

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

#### usePermissions Hook

```typescript
interface UsePermissionsReturn {
  pending: PermissionRequest | null;
  reply: (reply: 'allow' | 'deny' | 'always', message?: string) => Promise<void>;
  dismiss: () => void;
}

function usePermissions(sessionId: string | null): UsePermissionsReturn {
  const [pending, setPending] = useState<PermissionRequest | null>(null);

  // Subscribe to permission.asked events
  useEffect(() => {
    if (!sessionId) return;

    const unsubscribe = subscribeToAgentEvents((event) => {
      if (event.type === 'permission.asked') {
        setPending(event.properties.request);
      }
      if (event.type === 'permission.replied') {
        if (event.properties.requestId === pending?.id) {
          setPending(null);
        }
      }
    });

    return unsubscribe;
  }, [sessionId, pending?.id]);

  const reply = useCallback(async (
    replyType: 'allow' | 'deny' | 'always',
    message?: string
  ) => {
    if (!pending) return;

    await fetch(`/permission/${pending.id}/reply`, {
      method: 'POST',
      body: JSON.stringify({ reply: replyType, message }),
    });

    setPending(null);
  }, [pending]);

  const dismiss = useCallback(() => {
    if (pending) {
      reply('deny', 'Dismissed');
    }
  }, [pending, reply]);

  return { pending, reply, dismiss };
}
```

### 5. Integration with Thread View

```tsx
function ClaudeThreadView({ ... }) {
  const { pending, reply, dismiss } = usePermissions(sessionId);

  return (
    <div className="thread-view">
      <ThreadMessages messages={messages} />

      <PermissionModal
        request={pending}
        onReply={reply}
        onClose={dismiss}
      />

      <ThreadComposer
        onSubmit={handleSubmit}
        disabled={!!pending}  // Disable input while waiting for approval
      />
    </div>
  );
}
```

## Implementation Plan

### Phase 1: Server Infrastructure

1. Create `PermissionManager` class with pending request storage
2. Update `canUseTool` to use blocking Promise pattern
3. Add permission reply endpoint
4. Enhance SSE events with full request data

### Phase 2: Basic UI

1. Create `PermissionModal` component
2. Create `usePermissions` hook
3. Wire modal into `ClaudeThreadView`
4. Basic styling for all tool types

### Phase 3: Rich Context

1. Diff viewer for Edit operations
2. Command highlighting for Bash
3. URL preview for WebFetch
4. "Always allow" pattern storage

### Phase 4: Edge Cases

1. Handle abort during pending permission
2. Handle session disconnect/reconnect
3. Timeout for stale permissions
4. Queue multiple pending permissions

## Files to Create/Modify

| File | Action |
|------|--------|
| `daemon/claude-server/src/permissions/manager.ts` | NEW |
| `daemon/claude-server/src/permissions/types.ts` | NEW |
| `daemon/claude-server/src/routes/permissions.ts` | NEW |
| `daemon/claude-server/src/sdk/permissions.ts` | UPDATE (blocking) |
| `daemon/claude-server/src/types.ts` | UPDATE (PermissionRequest) |
| `daemon/claude-server/src/index.ts` | UPDATE (mount routes) |
| `app/src/features/claudecode/components/PermissionModal.tsx` | NEW |
| `app/src/features/claudecode/components/PermissionContext.tsx` | NEW |
| `app/src/features/claudecode/hooks/usePermissions.ts` | NEW |
| `app/src/features/claudecode/components/ClaudeThreadView.tsx` | UPDATE |
| `app/src/services/tauri.ts` | UPDATE (permission reply) |

## Timeout & Edge Cases

### Permission Timeout

```typescript
// In PermissionManager.request()
const PERMISSION_TIMEOUT_MS = 5 * 60 * 1000; // 5 minutes

setTimeout(() => {
  if (this.pending.get(sessionId)?.has(id)) {
    this.reply(sessionId, id, 'deny', 'Permission request timed out');
  }
}, PERMISSION_TIMEOUT_MS);
```

### Client Reconnection

When a client reconnects, fetch pending permissions:

```typescript
// On SSE connect
const pending = await fetch('/permission/pending?sessionId=' + sessionId);
if (pending.requests.length > 0) {
  setPending(pending.requests[0]);
}
```

### Abort Handling

If user aborts the session while permission is pending:

```typescript
signal.addEventListener('abort', () => {
  this.pending.get(sessionId)?.delete(id);
  reject(new Error('Session aborted'));
});
```

## Success Criteria

- [ ] `permissionMode: 'default'` pauses on dangerous tools
- [ ] UI displays permission modal with tool context
- [ ] User can Allow/Deny/Always Allow
- [ ] "Always" remembers patterns for session
- [ ] Abort cancels pending permissions
- [ ] Reconnecting client sees pending permissions
- [ ] Edit operations show diff preview
- [ ] Bash operations show command

## References

- OpenCode permission system: `external/opencode/packages/opencode/src/permission/next.ts`
- OpenCode TUI permission UI: `external/opencode/packages/opencode/src/cli/cmd/tui/routes/session/permission.tsx`
- Current Maestro permissions: `daemon/claude-server/src/sdk/permissions.ts`
- Claude SDK types: `daemon/claude-server/node_modules/@anthropic-ai/claude-code/sdk.d.ts`
