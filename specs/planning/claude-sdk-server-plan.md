# Claude SDK Server Implementation Plan

Reference: [claude-sdk-server.md](../claude-sdk-server.md)

---

## Testing Setup

Development happens on the VPS where the daemon runs. The Claude SDK server can be tested standalone or integrated with the running daemon.

### Running the Claude SDK Server Standalone (Phases 1-8)

```bash
cd daemon/claude-sdk-server
bun run src/index.ts --port 9100 --directory /path/to/test/project

# In another terminal
curl http://localhost:9100/health
curl http://localhost:9100/session
```

`ANTHROPIC_API_KEY` should already be set in the environment (required for Phase 4+).

### Testing with the Daemon (Phase 9+)

The daemon is already running on the VPS. Use the deploy script to restart after changes:

```bash
./scripts/deploy-daemon.sh restart
./scripts/deploy-daemon.sh logs  # Tail daemon logs
```

### Daemon Management

```bash
./scripts/deploy-daemon.sh status   # Check if running
./scripts/deploy-daemon.sh deploy   # Build + restart
./scripts/deploy-daemon.sh logs     # Tail logs
```

---

## Phase 1: Server Scaffold

Set up the Bun/TypeScript project structure and basic HTTP server.

- [x] Create `daemon/claude-sdk-server/` directory structure (§2)
- [x] Initialize Bun project with `package.json`, `tsconfig.json`
- [x] Add dependencies: `@anthropic-ai/claude-code`, `hono`, `uuid`
- [x] Implement `src/index.ts` with Hono HTTP server on configurable port
- [x] Add health check endpoint `GET /health`
- [x] Add JSON logging utility (§7)
- [x] Add graceful shutdown handler (SIGTERM, SIGINT)

**Verification:**
```bash
cd daemon/claude-sdk-server && bun install && bun run src/index.ts
curl http://localhost:9100/health  # → { "ok": true }
```

---

## Phase 2: Session CRUD

Implement session management endpoints without SDK integration.

- [x] Define types in `src/types.ts` (§3: Session, MessageInfo, Part)
- [x] Implement in-memory session store in `src/storage/sessions.ts`
- [x] Implement `GET /session` endpoint (§4)
- [x] Implement `POST /session` endpoint (§4)
- [x] Implement `GET /session/:id` endpoint (§4)
- [x] Add file persistence: save sessions to `~/.maestro/claude/{workspace_id}/` (§3)
- [x] Persist messages + parts and maintain `index.json` (§3)
- [x] Handle errors: SESSION_NOT_FOUND (404), validation errors (400) (§6)

**Verification:**
```bash
# Create session
curl -X POST http://localhost:9100/session -H 'Content-Type: application/json' \
  -d '{"title":"Test","permission":"acceptEdits"}'

# List sessions
curl http://localhost:9100/session

# Get session
curl http://localhost:9100/session/{id}
```

---

## Phase 3: SSE Event Stream

Implement Server-Sent Events infrastructure.

- [x] Create `src/events/emitter.ts` with EventEmitter for SSE broadcast
- [x] Implement `GET /event` SSE endpoint (§4)
- [x] Add client connection tracking (connect/disconnect)
- [x] Emit `session.created` when session created (§4)
- [x] Emit `session.updated` when session modified (§4)
- [x] Add keep-alive ping every 30s to prevent connection timeout

**Verification:**
```bash
# In terminal 1: listen to events
curl -N http://localhost:9100/event

# In terminal 2: create session (should see session.created event)
curl -X POST http://localhost:9100/session -H 'Content-Type: application/json' \
  -d '{"title":"Test","permission":"acceptEdits"}'
```

---

## Phase 4: SDK Integration (Core)

Wire up the Claude Agent SDK to handle messages.

- [x] Create `src/sdk/agent.ts` wrapper around `query()` (Appendix A)
- [x] Implement `POST /session/:id/message` endpoint (§4, §5 Main Flow)
- [x] Configure SDK with Claude Code options: cwd, permissionMode, model, resume (§10)
- [x] Pass `cwd`, `permissionMode`, `modelId` from session to SDK (§3, §10)
- [x] Handle SESSION_BUSY (409) for concurrent message attempts (§5 Edge Cases)
- [x] Emit `session.status { type: 'busy' }` on message start (§4)
- [x] Emit `session.status { type: 'idle' }` on completion (§4)

**Verification:**
```bash
# Create session
SESSION=$(curl -s -X POST http://localhost:9100/session \
  -H 'Content-Type: application/json' \
  -d '{"title":"Test","permission":"bypassPermissions"}' | jq -r .id)

# Send message (requires ANTHROPIC_API_KEY)
curl -X POST "http://localhost:9100/session/$SESSION/message" \
  -H 'Content-Type: application/json' \
  -d '{"parts":[{"type":"text","text":"What is 2+2?"}]}'
```

---

## Phase 5: Event Mapping

Map SDK messages to OpenCode-compatible events.

- [x] Create `src/events/mapper.ts` for SDK message → Part mapping (§3, Appendix B)
- [x] Map `text` content blocks to TextPart with delta streaming
- [ ] Map `thinking` content blocks to ReasoningPart
- [ ] Map `tool_use` to ToolPart with status transitions (pending → running)
- [ ] Map `tool_result` to ToolPart completion (status: completed/failed)
- [ ] Emit `step-start` part at turn start and `step-finish` at turn end (§3)
- [ ] Emit `message.updated` for message lifecycle (§4)
- [ ] Emit `message.part.updated` for each part/delta (§4)

**Verification:**
```bash
# Listen to events while sending a message that uses tools
# Should see: message.updated, message.part.updated (text),
#             message.part.updated (tool pending/running/completed)
```

---

## Phase 5.5: Contract Validation

Validate the SSE event envelope + ordering against OpenCode expectations.

- [ ] Capture a golden SSE transcript for a simple prompt and a tool prompt
- [ ] Verify payload shape is `{ type, properties }` and ordering matches spec
- [ ] Confirm `message.updated` precedes `message.part.updated` for same message

**Verification:**
```bash
# Manually inspect SSE transcript against expected OpenCode sequence
```

---

## Phase 6: Abort Support

Implement execution cancellation.

- [x] Track active SDK execution per session
- [x] Implement `POST /session/:id/abort` endpoint (§4, §5 Abort Flow)
- [x] Abort active SDK stream via AbortController
- [x] Emit `session.status { type: 'idle' }` after abort
- [x] Handle abort of non-busy session gracefully (no-op)

**Verification:**
```bash
# Start long-running task, then abort
curl -X POST "http://localhost:9100/session/$SESSION/abort"
```

---

## Phase 7: Session Resume

Support resuming previous conversations.

- [x] Persist `resumeId` from SDK result to session storage (§5 Resume Flow)
- [x] On `POST /session/:id/message`, load and pass `resumeId` to SDK
- [x] SDK automatically restores conversation context
- [ ] Verify resume works after server restart

**Verification:**
```bash
# Send message, stop server, restart, send another message
# Second message should have context from first
```

---

## Phase 8: Permission Events

Emit permission events for UI awareness (auto-approve for MVP).

- [ ] Create `src/sdk/permissions.ts` with auto-approve logic (§5 Permission Flow)
- [ ] Register SDK hooks: `PreToolUse`, `PostToolUse`, `PermissionRequest`
- [ ] Emit `permission.asked` event when SDK requests permission (§4)
- [ ] Auto-approve based on `permissionMode` setting
- [ ] Emit `permission.replied` event with decision (§4)

**Verification:**
```bash
# Send message that triggers tool use
# Should see permission.asked → permission.replied events
```

---

## Phase 9: Daemon Integration

Add daemon RPC commands to spawn/stop servers.

- [ ] Create `daemon/src/claude_server.rs` with spawn/stop logic (§4 Daemon RPC)
- [ ] Implement port allocation from configurable range (§10: 9100-9199)
- [ ] Track running servers: `workspace_id → (port, pid, status)`
- [ ] Implement `spawn_claude_server(workspace_id, directory)` → port
- [ ] Implement `stop_claude_server(workspace_id)`
- [ ] Implement `list_claude_servers()` → server info list
- [ ] Set `ANTHROPIC_API_KEY` env var on spawn
- [ ] Pass `workspace_id` + `directory` to server (args or env) for session metadata
- [ ] Capture server stderr for daemon logs
- [ ] Implement auto-restart once on crash (§10)

**Verification:**
```bash
# Terminal 1: Start daemon locally
cd daemon && cargo build --release
./target/release/maestro-daemon --listen 127.0.0.1:4733 --insecure-no-auth

# Terminal 2: Test spawn/stop via JSON-RPC (using nc or a test script)
# Send: {"jsonrpc":"2.0","method":"spawn_claude_server","params":{"workspace_id":"ws1","directory":"/tmp/test"},"id":1}
# Expect: {"jsonrpc":"2.0","result":{"port":9100},"id":1}

# Verify server is running
curl http://localhost:9100/health

# Stop server
# Send: {"jsonrpc":"2.0","method":"stop_claude_server","params":{"workspace_id":"ws1"},"id":2}
```

---

## Phase 10: Tauri Commands

Expose daemon Claude server commands to frontend.

- [ ] Add Tauri commands in `lib.rs`:
  - `claude_spawn_server(workspace_id, directory)` → port
  - `claude_stop_server(workspace_id)`
  - `claude_list_servers()`
- [ ] Add TypeScript wrappers in `services/tauri.ts`
- [ ] Wire to existing daemon client

**Verification:**
```bash
bun run typecheck  # No errors
```

---

## Phase 11: Frontend Integration [BLOCKED by: Phase 10]

Connect frontend to Claude SDK sessions.

- [ ] Create `useClaudeSession` hook that:
  - Spawns server via daemon
  - Connects to SSE stream
  - Maps events to existing ThreadView format
- [ ] Add Claude session type to session selector
- [ ] Verify ThreadView renders Claude sessions correctly

**Verification:**
- [ ]? Manual: Create Claude session in UI, send message, see streaming response

---

## Files to Create

- `daemon/claude-sdk-server/package.json`
- `daemon/claude-sdk-server/tsconfig.json`
- `daemon/claude-sdk-server/src/index.ts`
- `daemon/claude-sdk-server/src/types.ts`
- `daemon/claude-sdk-server/src/routes/sessions.ts`
- `daemon/claude-sdk-server/src/routes/messages.ts`
- `daemon/claude-sdk-server/src/routes/events.ts`
- `daemon/claude-sdk-server/src/sdk/agent.ts`
- `daemon/claude-sdk-server/src/sdk/hooks.ts`
- `daemon/claude-sdk-server/src/sdk/permissions.ts`
- `daemon/claude-sdk-server/src/events/emitter.ts`
- `daemon/claude-sdk-server/src/events/mapper.ts`
- `daemon/claude-sdk-server/src/storage/sessions.ts`
- `daemon/src/claude_server.rs` (or integrate into existing daemon module)

## Files to Modify

- `app/src-tauri/src/lib.rs` (add Tauri commands)
- `app/src-tauri/src/daemon/commands.rs` (add daemon RPC wrappers)
- `app/src/services/tauri.ts` (add TypeScript wrappers)

## Verification Checklist

### Implementation Checklist
- [x] `cd daemon/claude-sdk-server && bun run src/index.ts` starts without error
- [x] `curl http://localhost:9100/health` returns `{ "ok": true }`
- [x] `curl http://localhost:9100/session` returns `[]`
- [x] Session CRUD works via curl
- [x] SSE events stream correctly
- [ ] SDK query executes and streams events (requires API key)
- [ ] Abort stops execution
- [ ] Resume continues conversation
- [ ] Daemon spawn/stop commands work
- [ ] `bun run typecheck` passes in `app/`

### Manual QA Checklist (do not mark—human verification)
- [ ]? Create Claude session in Maestro UI
- [ ]? Send message, observe streaming text response
- [ ]? Send message triggering tool use, observe tool events
- [ ]? Abort mid-execution
- [ ]? Resume after app restart
- [ ]? Multiple concurrent workspaces with separate servers

---

## Notes

- **Phase 4 blocker**: Requires `ANTHROPIC_API_KEY` environment variable set.
- **Phase 9 dependency**: Daemon must be running for spawn/stop commands.
- **Phase 11 dependency**: Requires all previous phases complete.
- **Port conflicts**: If port range exhausted, spawn returns error. Consider expanding range or better cleanup.
- **SDK updates**: SDK behavior may change with Claude Code CLI updates. Pin versions if needed.
