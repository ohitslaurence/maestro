# Open Code Server Protocol Research

Research findings on sst/opencode's client/server architecture for Maestro integration.

## 1. Architecture Overview

### Server Component

Location: `packages/opencode/src/server/`

OpenCode provides **two server protocols**:

1. **HTTP REST API** (primary) - Hono-based HTTP server
   - Default port: 4096
   - Used by desktop app and web clients
   - OpenAPI documented at `/doc`

2. **ACP (Agent Client Protocol)** - JSON-RPC over stdio
   - Used for IDE integration (Zed, etc.)
   - Implements `@agentclientprotocol/sdk`
   - Started via `opencode acp`

### Deployment Model

```
┌─────────────────┐     HTTP/SSE      ┌─────────────────┐
│  Desktop App    │ ───────────────── │                 │
│  (Tauri)        │                   │                 │
└─────────────────┘                   │   OpenCode      │
                                      │   Server        │
┌─────────────────┐     HTTP/SSE      │   (Bun)         │
│  Web Client     │ ───────────────── │                 │
└─────────────────┘                   │                 │
                                      └─────────────────┘
┌─────────────────┐     JSON-RPC
│  IDE (Zed)      │ ───────────────── opencode acp (stdio)
└─────────────────┘
```

### Key Components

- **Instance**: Project context, provides working directory and state isolation
- **Session**: Conversation context with messages, parts, and tools
- **Bus**: Event pub/sub system for real-time updates
- **Storage**: Persistent storage for sessions, messages, parts

## 2. Protocol Details

### Transport Layer

**HTTP Server (primary)**
- Framework: Hono (Bun.serve)
- Default: `http://localhost:4096`
- Options: `--hostname`, `--port`
- mDNS: Optional discovery via Bonjour

**ACP Server (IDE integration)**
- JSON-RPC 2.0 over stdio
- Protocol version: v1
- Started via `opencode acp [--cwd /path]`

### Message Format

REST API with JSON payloads. OpenAPI 3.1.1 spec available at `/doc`.

### Request/Response Patterns

Standard REST:
- `GET /session` - List sessions
- `POST /session` - Create session
- `GET /session/:id` - Get session
- `POST /session/:id/message` - Send message (streaming response)
- `POST /session/:id/abort` - Cancel in-progress work

### Event/Notification Patterns

Server-Sent Events (SSE) at `GET /event`:

```typescript
// Connect to event stream
const eventSource = new EventSource('http://localhost:4096/event')

eventSource.onmessage = (event) => {
  const data = JSON.parse(event.data)
  // { type: "message.part.updated", properties: { part: {...}, delta: "..." } }
}
```

## 3. Session Management

### Session Lifecycle

```typescript
// Create session
POST /session
{ parentID?: string, title?: string }
-> Session

// Get session
GET /session/:sessionID
-> Session

// List sessions
GET /session?directory=...&roots=true&limit=10
-> Session[]

// Delete session
DELETE /session/:sessionID
-> boolean

// Fork session at message
POST /session/:sessionID/fork
{ messageID?: string }
-> Session
```

### Session Schema

```typescript
interface Session {
  id: string              // "session_01..."
  slug: string            // human-readable slug
  projectID: string       // project identifier
  directory: string       // working directory
  parentID?: string       // for child sessions
  title: string
  version: string         // opencode version
  time: {
    created: number
    updated: number
    compacting?: number
    archived?: number
  }
  share?: { url: string }
  revert?: { messageID: string, snapshot?: string }
}
```

### Session States

From `SessionStatus`:

```typescript
type SessionStatus =
  | { type: "idle" }
  | { type: "busy" }
  | { type: "retry", attempt: number, message: string, next: number }
```

## 4. Event Streaming

### Connection

```typescript
GET /event
Content-Type: text/event-stream
```

Initial events:
1. `server.connected` - Connection established
2. `server.heartbeat` - Every 30s to prevent timeout

### Event Types

**Session Events**
- `session.created` - New session
- `session.updated` - Session modified
- `session.deleted` - Session removed
- `session.status` - Status change (idle/busy/retry)
- `session.idle` - Session became idle
- `session.error` - Error occurred
- `session.diff` - File changes summary

**Message Events**
- `message.updated` - Message info changed
- `message.removed` - Message deleted
- `message.part.updated` - Part updated (with delta for streaming)
- `message.part.removed` - Part deleted

**Permission Events**
- `permission.updated` - Permission request
- `permission.replied` - Permission response

**PTY Events**
- `pty.created` / `pty.updated` / `pty.exited` / `pty.deleted`

**Other Events**
- `todo.updated` - Task list changed
- `file.edited` - File modified
- `file.watcher.updated` - File system change
- `vcs.branch.updated` - Git branch changed
- `installation.updated` - Version updated

### Event Payload Structure

```typescript
interface Event {
  type: string
  properties: Record<string, unknown>
}

// Example: streaming text
{
  type: "message.part.updated",
  properties: {
    part: { id: "part_01...", type: "text", text: "Hello" },
    delta: " world"  // incremental text
  }
}
```

## 5. Authentication

### Basic Auth

```bash
# Environment variables
OPENCODE_SERVER_PASSWORD=secret
OPENCODE_SERVER_USERNAME=opencode  # optional, defaults to "opencode"
```

```typescript
// Client request
headers: {
  Authorization: `Basic ${btoa('opencode:secret')}`
}
```

### CORS Policy

Allowed origins:
- `http://localhost:*`
- `http://127.0.0.1:*`
- `tauri://localhost`
- `http://tauri.localhost`
- `https://*.opencode.ai`
- Custom whitelist via `--cors` flag

## 6. Integration Options for Maestro

### Option A: HTTP REST Client

**Pros:**
- Well-documented OpenAPI spec
- Generated TypeScript SDK available
- SSE for real-time events
- Same protocol as desktop app

**Cons:**
- Requires spawning/managing server process
- Network overhead vs stdio

**Implementation:**

```typescript
import { createOpencodeClient, createOpencodeServer } from '@opencode-ai/sdk'

// Start server
const server = await createOpencodeServer({
  hostname: '127.0.0.1',
  port: 4096,
})

// Create client
const client = createOpencodeClient({
  baseUrl: server.url,
  headers: { Authorization: `Basic ${btoa('opencode:secret')}` }
})

// Create session
const session = await client.session.sessionCreate()

// Subscribe to events
const events = new EventSource(`${server.url}/event`)
events.onmessage = (e) => handleEvent(JSON.parse(e.data))

// Send message
await client.session.sessionPrompt({
  path: { sessionID: session.id },
  body: {
    parts: [{ type: 'text', text: 'Hello' }],
    providerID: 'anthropic',
    modelID: 'claude-sonnet-4-20250514'
  }
})
```

### Option B: ACP (Agent Client Protocol)

**Pros:**
- Stdio-based, simpler process management
- Protocol designed for IDE integration
- No network configuration

**Cons:**
- Less mature than HTTP API
- Missing features (streaming, tool visibility)
- Single session per process

**Implementation:**

```typescript
import { spawn } from 'child_process'

const proc = spawn('opencode', ['acp', '--cwd', projectDir])

// JSON-RPC over stdio
proc.stdin.write(JSON.stringify({
  jsonrpc: '2.0',
  id: 1,
  method: 'initialize',
  params: { protocolVersion: 1 }
}) + '\n')
```

### Option C: Direct Module Import

**Pros:**
- No subprocess overhead
- Full access to internals
- Maximum control

**Cons:**
- Tight coupling to OpenCode internals
- Bun runtime dependency
- Breaking changes risk

## 7. Recommendations

### Primary Approach: HTTP REST API

1. **Spawn server as subprocess**
   - Use SDK's `createOpencodeServer()` helper
   - Or spawn `opencode serve` directly
   - Manage lifecycle (start/stop/restart)

2. **Use generated SDK client**
   - Located at `packages/sdk/js/`
   - Auto-generated from OpenAPI spec
   - Type-safe operations

3. **Subscribe to SSE events**
   - Single connection for all events
   - Receive streaming deltas
   - Handle reconnection

### Key Differences from Claude Code

| Aspect | OpenCode | Claude Code |
|--------|----------|-------------|
| Protocol | HTTP REST + SSE | CLI process + stdout parsing |
| Events | Structured JSON via SSE | Markdown in stdout |
| Session | Server-managed | File-based |
| SDK | Generated TypeScript | None (CLI only) |
| Authentication | Basic auth | N/A (local) |

### Code Structure to Reference

```
packages/opencode/src/
├── server/           # HTTP server
│   ├── server.ts     # Main server setup
│   └── routes/       # API routes
├── session/          # Session management
├── bus/              # Event system
└── acp/              # ACP protocol

packages/sdk/js/      # Generated SDK
├── src/gen/          # Auto-generated types
└── src/client.ts     # Client wrapper
```

### Gaps/Limitations

1. **No official public API stability** - OpenCode is under active development
2. **Bun dependency** - Server requires Bun runtime
3. **Session isolation** - Multiple directories need separate instances
4. **Permission handling** - Client must respond to permission requests
5. **ACP incomplete** - HTTP API is more mature

### Recommended Integration Architecture

```
┌─────────────────────────────────────────────────────┐
│                    Maestro                          │
├─────────────────────────────────────────────────────┤
│  OpenCodeAdapter                                    │
│  ├─ spawn/manage opencode serve                     │
│  ├─ HTTP client (SDK)                               │
│  ├─ SSE event stream                                │
│  └─ session state machine                           │
├─────────────────────────────────────────────────────┤
│  AgentInterface (shared with Claude Code)           │
│  ├─ send(message)                                   │
│  ├─ cancel()                                        │
│  ├─ onEvent(callback)                               │
│  └─ getState()                                      │
└─────────────────────────────────────────────────────┘
```
