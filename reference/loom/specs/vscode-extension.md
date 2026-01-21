<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# VS Code Extension (loom-vscode)

## Overview

The `loom-vscode` extension provides VS Code integration for Loom via the Agent Client Protocol (ACP).
It acts as an ACP Client, spawning `loom acp-agent` as a subprocess and communicating over stdio using
JSON-RPC to provide an AI coding assistant experience within VS Code.

### Architecture

```
VS Code Window
┌────────────────────────────────────────────────────────────────┐
│ Extension Host (Node)                                          │
│                                                                │
│  ┌────────────────────┐     ┌──────────────────────────┐       │
│  │ AgentProcessMgr    │     │  ACP Client (AcpClient)  │       │
│  │ - spawn loom       │◄───▶│  ClientSideConnection    │       │
│  │ - restart/backoff  │  stdio JSON-RPC               │       │
│  └────────────────────┘     └──────────────────────────┘       │
│           │                             │                      │
│           ▼                             ▼                      │
│  ┌────────────────────┐      ┌────────────────────────┐        │
│  │ SessionManager     │◄────▶│ ChatController         │        │
│  │ - SessionId <-> UI │      │ - commands, routing    │        │
│  │ - persistence      │      └────────────────────────┘        │
│           │                             │                      │
│           ▼                             ▼                      │
│  ┌────────────────────┐      ┌────────────────────────┐        │
│  │ VS Code Workspace  │      │ Chat Webview           │        │
│  │ - root path / CWD  │      │ - render chat          │        │
│  │ - open files       │      │ - stream responses     │        │
│  └────────────────────┘      └────────────────────────┘        │
└────────────────────────────────────────────────────────────────┘

Subprocess
┌──────────────────────────────────────────────────────────────────┐
│ `loom acp-agent`                                                 │
│ - LoomAcpAgent (Rust)                                            │
│ - Tool execution (edit_file, read_file, bash, etc.)             │
│ - Thread persistence                                             │
│ - LLM via loom-llm-proxy                                         │
└──────────────────────────────────────────────────────────────────┘
```

## Directory Structure

```
ide/vscode/
├── package.json              # Extension manifest
├── tsconfig.json             # TypeScript configuration
├── webpack.config.js         # Bundle configuration
├── .vscodeignore             # Files to exclude from packaging
├── README.md                 # Extension README
├── CHANGELOG.md              # Version history
├── src/
│   ├── extension.ts          # Activation, wiring singletons
│   ├── logging.ts            # Centralized logger (OutputChannel)
│   ├── config/
│   │   └── configService.ts  # Read & watch VS Code settings
│   ├── workspace/
│   │   └── workspaceService.ts  # Workspace root, selection helpers
│   ├── acp/
│   │   ├── agentProcessManager.ts  # Spawn & manage loom acp-agent
│   │   ├── acpClient.ts            # Wraps ClientSideConnection
│   │   └── types.ts                # Local typed wrappers
│   ├── sessions/
│   │   └── sessionManager.ts    # Session state + persistence
│   ├── chat/
│   │   ├── chatController.ts    # Commands, prompt flow
│   │   ├── chatViewProvider.ts  # WebviewViewProvider implementation
│   │   └── models.ts            # ChatMessage, ToolCallViewModel, etc.
│   └── util/
│       └── asyncUtils.ts        # Debounce, timeout wrappers
├── media/
│   ├── chat.html             # Webview HTML shell
│   ├── chat.css              # Webview styles
│   └── chat.js               # Webview JavaScript (compiled)
├── webview-ui/
│   ├── src/
│   │   ├── main.ts           # Webview entry point
│   │   ├── components/       # UI components
│   │   └── state/            # Webview state management
│   └── tsconfig.json
└── test/
    ├── runTest.ts
    ├── suite/
    │   ├── extension.test.ts
    │   ├── sessionManager.test.ts
    │   └── acpClient.test.ts
    └── fixtures/
        └── fakeAcpAgent.ts   # Test agent implementation
```

## Core Components

### 1. AgentProcessManager

Manages the lifecycle of the `loom acp-agent` subprocess.

```typescript
interface AgentProcessManagerOptions {
  loomPath?: string;           // Path to loom binary (default: 'loom' from PATH)
  additionalArgs?: string[];   // Extra CLI arguments
  workspaceRoot: string;       // Working directory for the process
}

interface AgentProcessManager {
  // Start the agent process
  start(): Promise<ChildProcess>;
  
  // Stop the agent process gracefully
  stop(): Promise<void>;
  
  // Restart with exponential backoff
  restart(): Promise<void>;
  
  // Get the current process (if running)
  getProcess(): ChildProcess | undefined;
  
  // Events
  onExit: Event<{ code: number | null; signal: string | null }>;
  onError: Event<Error>;
  onReady: Event<void>;
}
```

**Implementation Details:**

- Spawn `loom acp-agent` with:
  - `stdio: ['pipe', 'pipe', 'pipe']`
  - `cwd` = workspace root
- Automatic restart on unexpected exit with exponential backoff (1s, 2s, 4s, max 30s)
- Log stderr to OutputChannel for debugging
- Emit `onReady` once stdio pipes are established

### 2. AcpClient

Wraps `@agentclientprotocol/sdk`'s `ClientSideConnection` for Loom-specific usage.

```typescript
interface AcpClient {
  // Initialize the ACP connection
  initialize(): Promise<InitializeResponse>;
  
  // Create a new session
  newSession(cwd: string): Promise<SessionId>;
  
  // Load an existing session
  loadSession(sessionId: string): Promise<void>;
  
  // Send a prompt and await completion
  prompt(sessionId: string, content: ContentBlock[]): Promise<PromptResponse>;
  
  // Cancel an ongoing prompt
  cancel(sessionId: string): Promise<void>;
  
  // Events
  onSessionUpdate: Event<SessionNotification>;
  onError: Event<AcpError>;
  onInitialized: Event<AgentCapabilities>;
  onDisconnected: Event<void>;
}
```

**Protocol Flow:**

1. On process ready → create `ClientSideConnection` from stdio streams
2. Call `initialize()` with client info and capabilities
3. Ready to handle `session/new`, `session/load`, `session/prompt`
4. Handle incoming `session/update` notifications and emit events

### 3. SessionManager

Manages the mapping between VS Code UI sessions and ACP SessionIds (which equal Loom ThreadIds).

```typescript
interface LoomSession {
  id: string;              // SessionId == ThreadId
  title: string;           // UI display name
  createdAt: number;       // Unix timestamp
  lastUsedAt: number;      // Unix timestamp
  isActive: boolean;       // Currently selected
}

interface SessionManager {
  // Get the currently active session
  getActiveSession(): LoomSession | undefined;
  
  // Get all sessions for this workspace
  getAllSessions(): LoomSession[];
  
  // Create a new session
  createNewSession(): Promise<LoomSession>;
  
  // Switch to a different session
  setActiveSession(sessionId: string): Promise<void>;
  
  // Update session metadata (e.g., title from first message)
  updateSessionTitle(sessionId: string, title: string): void;
  
  // Delete a session from local tracking
  deleteSession(sessionId: string): void;
}
```

**Persistence:**

Sessions are persisted to VS Code's `workspaceState`:

```typescript
interface WorkspaceSessionsState {
  activeSessionId?: string;
  sessions: LoomSession[];
}
```

### 4. ChatController

Orchestrates the chat flow between the webview UI and ACP client.

```typescript
interface ChatController {
  // Handle user message from webview
  handleUserMessage(text: string, context?: MessageContext): Promise<void>;
  
  // Cancel current operation
  cancelCurrentTurn(): Promise<void>;
  
  // Switch sessions
  switchSession(sessionId: string): Promise<void>;
  
  // Create new session and switch to it
  newSession(): Promise<void>;
  
  // Get conversation history for current session
  getConversationHistory(): ChatMessage[];
}

interface MessageContext {
  includeSelection?: boolean;  // Include current editor selection
  filePath?: string;           // Active file path
  selectionText?: string;      // Selected text content
  selectionRange?: Range;      // Selection line/col range
}
```

**Responsibilities:**

- Translate webview messages to ACP calls
- Build `ContentBlock[]` from user input + context
- Handle streaming `session/update` events and update UI
- Maintain in-memory conversation history per session
- Map errors to user-friendly messages

### 5. ChatViewProvider

Implements `vscode.WebviewViewProvider` for the chat panel.

```typescript
class LoomChatViewProvider implements WebviewViewProvider {
  public static readonly viewType = 'loom.chatView';
  
  resolveWebviewView(
    webviewView: WebviewView,
    context: WebviewViewResolveContext,
    token: CancellationToken
  ): void;
  
  // Post message to webview
  postMessage(message: WebviewMessage): void;
  
  // Handle message from webview
  private handleWebviewMessage(message: WebviewMessage): void;
}
```

## Extension Configuration

### package.json Settings

```json
{
  "contributes": {
    "configuration": {
      "title": "Loom",
      "properties": {
        "loom.loomPath": {
          "type": "string",
          "default": "",
          "description": "Path to loom binary (defaults to 'loom' in PATH)"
        },
        "loom.additionalArgs": {
          "type": "array",
          "items": { "type": "string" },
          "default": [],
          "description": "Additional arguments to pass to loom acp-agent"
        },
        "loom.logLevel": {
          "type": "string",
          "enum": ["error", "warn", "info", "debug", "trace"],
          "default": "info",
          "description": "Log level for Loom extension"
        },
        "loom.autoStart": {
          "type": "boolean",
          "default": true,
          "description": "Automatically start Loom agent when opening chat"
        },
        "loom.serverUrl": {
          "type": "string",
          "default": "",
          "description": "URL of the Loom server. If empty, starts a local agent process."
        }
      }
    }
  }
}
```

### Commands

```json
{
  "contributes": {
    "commands": [
      {
        "command": "loom.openChat",
        "title": "Loom: Open Chat"
      },
      {
        "command": "loom.newSession",
        "title": "Loom: New Session"
      },
      {
        "command": "loom.cancelCurrentTurn",
        "title": "Loom: Cancel Current Operation"
      },
      {
        "command": "loom.restartAgent",
        "title": "Loom: Restart Agent"
      },
      {
        "command": "loom.showLogs",
        "title": "Loom: Show Logs"
      },
      {
        "command": "loom.explainSelection",
        "title": "Loom: Explain Selection"
      },
      {
        "command": "loom.refactorSelection",
        "title": "Loom: Refactor Selection"
      }
    ]
  }
}
```

### Views

```json
{
  "contributes": {
    "viewsContainers": {
      "activitybar": [
        {
          "id": "loom",
          "title": "Loom",
          "icon": "media/loom-icon.svg"
        }
      ]
    },
    "views": {
      "loom": [
        {
          "type": "webview",
          "id": "loom.chatView",
          "name": "Chat"
        }
      ]
    }
  }
}
```

### Activation Events

```json
{
  "activationEvents": [
    "onCommand:loom.openChat",
    "onView:loom.chatView",
    "onCommand:loom.explainSelection",
    "onCommand:loom.refactorSelection"
  ]
}
```

## Webview UI Design

### Chat Panel Layout

```
┌────────────────────────────────────────┐
│ Header                                 │
│  [🟢 Connected] Session: Bug fix #123  │
│  [New Session ▾] [⚙️]                  │
├────────────────────────────────────────┤
│ Messages (scrollable)                  │
│                                        │
│  ┌──────────────────────────────────┐  │
│  │ 👤 User                     12:34│  │
│  │ Can you help me fix this bug?   │  │
│  └──────────────────────────────────┘  │
│                                        │
│  ┌──────────────────────────────────┐  │
│  │ 🤖 Loom                    12:34│  │
│  │ I'll analyze the code...        │  │
│  │                                  │  │
│  │ ┌────────────────────────────┐  │  │
│  │ │ 🔧 read_file              │  │  │
│  │ │ src/api.rs ✓ completed    │  │  │
│  │ └────────────────────────────┘  │  │
│  │                                  │  │
│  │ ┌────────────────────────────┐  │  │
│  │ │ 🔧 edit_file              │  │  │
│  │ │ src/api.rs ⏳ running...   │  │  │
│  │ └────────────────────────────┘  │  │
│  │                                  │  │
│  │ Here's the fix: [streaming...]  │  │
│  └──────────────────────────────────┘  │
│                                        │
├────────────────────────────────────────┤
│ Context Strip                          │
│  [✓] Include selection from api.rs     │
├────────────────────────────────────────┤
│ Input                                  │
│ ┌──────────────────────────────────┐   │
│ │ Type your message...             │   │
│ │                                  │   │
│ └──────────────────────────────────┘   │
│         [Send (Ctrl+Enter)] [Cancel]   │
└────────────────────────────────────────┘
```

### Webview Message Protocol

**Extension → Webview:**

```typescript
type ExtensionToWebviewMessage =
  | { type: 'connectionStatus'; status: 'connecting' | 'connected' | 'disconnected' | 'error'; error?: string }
  | { type: 'sessionChanged'; session: LoomSession }
  | { type: 'sessionsUpdated'; sessions: LoomSession[] }
  | { type: 'messageAdded'; message: ChatMessage }
  | { type: 'messageUpdated'; messageId: string; content: Partial<ChatMessage> }
  | { type: 'streamingChunk'; messageId: string; text: string }
  | { type: 'toolCallUpdate'; messageId: string; toolCall: ToolCallStatus }
  | { type: 'turnCompleted'; messageId: string; stopReason: StopReason }
  | { type: 'error'; error: string };
```

**Webview → Extension:**

```typescript
type WebviewToExtensionMessage =
  | { type: 'ready' }
  | { type: 'sendMessage'; text: string; includeSelection: boolean }
  | { type: 'cancel' }
  | { type: 'newSession' }
  | { type: 'switchSession'; sessionId: string }
  | { type: 'deleteSession'; sessionId: string }
  | { type: 'copyCode'; code: string }
  | { type: 'insertCode'; code: string }
  | { type: 'openFile'; path: string; line?: number };
```

### ChatMessage Model

```typescript
interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: number;
  isStreaming?: boolean;
  toolCalls?: ToolCallStatus[];
  stopReason?: StopReason;
  error?: string;
}

interface ToolCallStatus {
  id: string;
  toolName: string;
  arguments?: Record<string, unknown>;
  status: 'pending' | 'running' | 'completed' | 'failed';
  result?: string;
  error?: string;
}
```

## Extension Lifecycle

### Activation Flow

```
1. VS Code starts extension (via activation event)
   │
2. Register components:
   ├─ ConfigService (read settings)
   ├─ WorkspaceService (get workspace root)
   ├─ AgentProcessManager (but don't start yet)
   ├─ AcpClient (wrapper only)
   ├─ SessionManager (load from workspaceState)
   ├─ ChatController
   └─ LoomChatViewProvider
   │
3. Register commands and views
   │
4. Wait for first interaction...
   │
5. User opens chat view (onView:loom.chatView)
   │
6. Start agent process:
   ├─ AgentProcessManager.start()
   ├─ Wait for process ready
   └─ AcpClient.initialize()
   │
7. Initialize session:
   ├─ Check workspaceState for activeSessionId
   ├─ If exists: session/load
   └─ If not: session/new
   │
8. Ready for prompts
```

### Deactivation Flow

```
1. VS Code signals deactivation
   │
2. Cancel any in-flight operations
   │
3. Stop agent process gracefully
   │
4. Save session state to workspaceState
   │
5. Dispose all subscriptions
```

## Error Handling

### Error Categories

| Category | Handling |
|----------|----------|
| Binary not found | Show error notification with link to settings |
| Process crash | Show notification with [Restart] action |
| ACP protocol error | Show inline in chat as error message |
| Session not found | Auto-create new session, notify user |
| Timeout | Show timeout error, offer cancel/retry |
| Version mismatch | Show prominent error, suggest upgrade |

### Error Recovery Strategies

1. **Process crash recovery:**
   - Automatic restart with exponential backoff
   - Preserve session IDs for re-loading
   - Show user-friendly notification

2. **Network/timeout errors:**
   - Retry transient errors up to 3 times
   - Show progress indicator during retries
   - Allow user to cancel and retry manually

3. **Session desync:**
   - If `session/load` fails, auto-create new session
   - Notify user: "Previous session could not be loaded"
   - Log original session ID for debugging

## Testing Strategy

### Unit Tests

- **SessionManager:** Creation, persistence, switching
- **ChatController:** Message handling, context building
- **ConfigService:** Settings parsing and defaults
- **Error handling:** Error type to message mapping

### Integration Tests

Using a fake ACP agent implementation:

```typescript
// test/fixtures/fakeAcpAgent.ts
class FakeAcpAgent implements Agent {
  async initialize(req: InitializeRequest): Promise<InitializeResponse> { ... }
  async newSession(req: NewSessionRequest): Promise<NewSessionResponse> { ... }
  async prompt(req: PromptRequest): Promise<PromptResponse> {
    // Echo back prompt as streaming chunks
    // Emit synthetic tool calls
  }
}
```

Test scenarios:
- Full prompt round-trip
- Streaming response handling
- Tool call visualization
- Cancel mid-stream
- Error handling

### Manual Testing Checklist

- [ ] Extension activates on command
- [ ] Agent process starts successfully
- [ ] New session creates correctly
- [ ] Prompt streams response in real-time
- [ ] Tool calls display with status updates
- [ ] Cancel stops ongoing operation
- [ ] Session persists across VS Code restarts
- [ ] Process crash triggers restart
- [ ] Settings changes apply correctly

## Dependencies

### Runtime Dependencies

```json
{
  "dependencies": {
    "@agentclientprotocol/sdk": "^0.12.0"
  }
}
```

### Dev Dependencies

```json
{
  "devDependencies": {
    "@types/vscode": "^1.85.0",
    "@types/node": "^20.0.0",
    "typescript": "^5.3.0",
    "webpack": "^5.89.0",
    "webpack-cli": "^5.1.0",
    "ts-loader": "^9.5.0",
    "@vscode/test-electron": "^2.3.0",
    "mocha": "^10.2.0",
    "@types/mocha": "^10.0.0"
  }
}
```

## Future Enhancements

### Phase 2: Enhanced Integration

1. **ACP fs/* handlers:** Delegate file operations to VS Code's workspace.fs
2. **ACP terminal handlers:** Use VS Code's integrated terminal
3. **Inline diff preview:** Show proposed edits before applying
4. **Code actions:** Quick fixes and refactorings from assistant suggestions

### Phase 3: Advanced Features

1. **Multi-agent support:** Connect to different Loom instances
2. **Full history sync:** Load complete thread history from Loom
3. **VS Code Chat API integration:** Participate in VS Code's chat ecosystem
4. **Collaborative sessions:** Share sessions between team members

## Security Considerations

1. **Process isolation:** Agent runs as a subprocess with limited privileges
2. **No credential storage:** Extension doesn't handle API keys (Loom manages via server)
3. **Workspace scope:** Each session is scoped to workspace root
4. **CSP for webview:** Strict Content Security Policy prevents script injection

## Performance Guidelines

1. **Batch UI updates:** Throttle streaming updates at 50-100ms intervals
2. **Virtual scrolling:** For long conversations, use virtual list rendering
3. **Lazy loading:** Don't load full history until needed
4. **Process pooling:** One agent process per VS Code window (not per workspace folder)
