# Claude Code SDK Research

Research findings for integrating Claude Code agents into Maestro.

## 1. SDK Overview

### Available Packages

| Language | Package | Repository |
|----------|---------|------------|
| TypeScript | `@anthropic-ai/claude-agent-sdk` | [anthropics/claude-agent-sdk-typescript](https://github.com/anthropics/claude-agent-sdk-typescript) |
| Python | `claude-agent-sdk` | [anthropics/claude-agent-sdk-python](https://github.com/anthropics/claude-agent-sdk-python) |

Both SDKs require Claude Code CLI as runtime:
```bash
curl -fsSL https://claude.ai/install.sh | bash
```

### Naming

The Claude Code SDK is now branded as the **Claude Agent SDK** in docs and packages. The runtime is still the Claude Code CLI.

### Core Concepts

1. **Agent SDK vs Client SDK**: The Agent SDK handles tool execution loop autonomously; the Client SDK requires manual implementation.

2. **SDK vs CLI**: The SDK provides programmatic control; the CLI is the underlying runtime process.

3. **Built-in Tools**:
   - `Read`, `Write`, `Edit` - File operations
   - `Bash` - Command execution
   - `Glob`, `Grep` - File/content search
   - `WebSearch`, `WebFetch` - Web operations
   - `Task` - Subagent spawning
   - `TodoWrite` - Task tracking

4. **Settings Loading**: SDK does **not** load filesystem settings by default. You must opt in with `settingSources` to load `CLAUDE.md`, `.claude/settings.json`, skills, and slash commands.

5. **Claude Code Presets**: Use `systemPrompt: { type: 'preset', preset: 'claude_code' }` and `tools: { type: 'preset', preset: 'claude_code' }` to mirror CLI defaults.

6. **MCP Integration**: Full Model Context Protocol support for custom tools.

7. **Hooks System**: Event interception at PreToolUse, PostToolUse, etc.

---

## 2. SDK vs CLI Behavior

### Runtime Relationship

- The Agent SDK shells out to the installed `claude` CLI binary.
- CLI updates can change SDK runtime behavior even if the SDK package version is unchanged.
- SDK API compatibility is controlled by the SDK package version; execution semantics are controlled by the CLI.

### Default Behavior Differences

| Area | CLI | SDK Default | SDK Override to Match CLI |
|------|-----|-------------|---------------------------|
| Settings | Loads user/project/local settings | Loads **none** | `settingSources: ['user','project','local']` |
| System prompt | Claude Code preset | Minimal prompt | `systemPrompt: { type: 'preset', preset: 'claude_code' }` |
| Tools | Claude Code default toolset | All tools unless limited | `tools: { type: 'preset', preset: 'claude_code' }` |
| Skills/Slash commands | Enabled via `.claude/` | Disabled unless settings loaded | `settingSources` + preset system prompt |
| Auth | Claude.ai / Console login | API key only | Set `ANTHROPIC_API_KEY` (or Bedrock/Vertex/Foundry envs) |

### Headless CLI Mode

`claude -p` is the supported programmatic CLI path. It supports JSON/stream output, structured outputs via JSON Schema, and the same flags as interactive mode.

---

## 3. Agent Lifecycle

### TypeScript: `query()` Function

```typescript
import { query } from '@anthropic-ai/claude-agent-sdk';

// Single prompt - returns async generator
for await (const message of query({
  prompt: "Find and fix bugs in auth.py",
  options: {
    allowedTools: ["Read", "Edit", "Bash"],
    permissionMode: "acceptEdits",
    cwd: "/project",
    maxTurns: 10
  }
})) {
  console.log(message);
}
```

### Python: `query()` Function

```python
from claude_agent_sdk import query, ClaudeAgentOptions

options = ClaudeAgentOptions(
    allowed_tools=["Read", "Edit", "Bash"],
    permission_mode="acceptEdits",
    cwd="/project"
)

async for message in query(prompt="Fix the bug", options=options):
    print(message)
```

### Python: `ClaudeSDKClient` (Session-based)

```python
from claude_agent_sdk import ClaudeSDKClient

async with ClaudeSDKClient() as client:
    # First exchange
    await client.query("What files are in this directory?")
    async for msg in client.receive_response():
        print(msg)

    # Follow-up (maintains context)
    await client.query("Read the main one")
    async for msg in client.receive_response():
        print(msg)
```

### Session Management

| Feature | `query()` | `ClaudeSDKClient` |
|---------|-----------|-------------------|
| Session | New each time | Reuses same session |
| Context | Single exchange | Multi-turn conversation |
| Interrupts | Not supported | Supported |
| Hooks | Not supported (Python) | Supported |

### Spawning Agents

```typescript
// TypeScript
const result = query({
  prompt: "Task description",
  options: {
    maxTurns: 20,
    maxBudgetUsd: 1.0,
    model: "claude-sonnet-4-20250514",
    cwd: "/workspace",
    permissionMode: "bypassPermissions"
  }
});
```

### Stopping/Canceling

```typescript
// TypeScript - via AbortController
const controller = new AbortController();
const result = query({
  prompt: "Long task",
  options: { abortController: controller }
});

// Later: cancel
controller.abort();
```

```python
# Python - via ClaudeSDKClient
async with ClaudeSDKClient() as client:
    await client.query("Long task")
    await client.interrupt()  # Stop execution
```

### Resuming Sessions

```typescript
// Resume a previous session by ID
const result = query({
  prompt: "Continue working",
  options: {
    resume: "session-id-here",
    forkSession: false  // true to fork to new session
  }
});
```

---

## 4. Event Model

### Message Types

```typescript
type SDKMessage =
  | SDKAssistantMessage    // Claude's response
  | SDKUserMessage         // User input
  | SDKSystemMessage       // System init
  | SDKResultMessage       // Final result
  | SDKPartialAssistantMessage  // Streaming (if enabled)
  | SDKCompactBoundaryMessage;  // Compaction marker
```

### SDKAssistantMessage Structure

```typescript
type SDKAssistantMessage = {
  type: 'assistant';
  uuid: string;
  session_id: string;
  message: {
    role: 'assistant';
    content: ContentBlock[];
  };
  parent_tool_use_id: string | null;
}
```

### Content Blocks

```typescript
type ContentBlock =
  | TextBlock           // { type: 'text', text: string }
  | ThinkingBlock       // { type: 'thinking', thinking: string }
  | ToolUseBlock        // { type: 'tool_use', id, name, input }
  | ToolResultBlock;    // { type: 'tool_result', tool_use_id, content }
```

### SDKResultMessage (Final)

```typescript
type SDKResultMessage = {
  type: 'result';
  subtype: 'success' | 'error_max_turns' | 'error_during_execution' | 'error_max_budget_usd';
  session_id: string;
  duration_ms: number;
  duration_api_ms: number;
  is_error: boolean;
  num_turns: number;
  result: string;           // Final text result
  total_cost_usd: number;
  usage: Usage;
  modelUsage: Record<string, ModelUsage>;
  permission_denials: SDKPermissionDenial[];
};
```

### Streaming Partial Messages

Enable with `includePartialMessages: true`:

```typescript
const result = query({
  prompt: "...",
  options: { includePartialMessages: true }
});

for await (const message of result) {
  if (message.type === 'stream_event') {
    // Raw Anthropic API stream events
    console.log(message.event);
  }
}
```

### Hook Events

```typescript
type HookEvent =
  | 'PreToolUse'        // Before tool execution
  | 'PostToolUse'       // After tool execution
  | 'PostToolUseFailure' // After tool failure
  | 'UserPromptSubmit'  // On prompt submission
  | 'SessionStart'      // Session begins
  | 'SessionEnd'        // Session ends
  | 'Stop'              // Agent stopping
  | 'SubagentStart'     // Subagent spawned
  | 'SubagentStop'      // Subagent complete
  | 'PreCompact'        // Before context compaction
  | 'Notification'      // Notification sent
  | 'PermissionRequest'; // Permission needed
```

### Hook Example

```typescript
const result = query({
  prompt: "...",
  options: {
    hooks: {
      PreToolUse: [{
        matcher: "Bash",  // Only for Bash tool
        hooks: [async (input, toolUseId, { signal }) => {
          console.log(`About to run: ${input.tool_input.command}`);
          // Return to block: { decision: 'block', reason: '...' }
          return {};
        }]
      }],
      PostToolUse: [{
        hooks: [async (input, toolUseId, ctx) => {
          console.log(`Tool ${input.tool_name} completed`);
          return {};
        }]
      }]
    }
  }
});
```

---

## 5. Integration Options for Maestro

### Option A: Use SDK Directly (Recommended)

**Approach**: Import `@anthropic-ai/claude-agent-sdk` in Maestro's Rust backend via Node subprocess or use Python SDK.

**Pros**:
- Full programmatic control
- Official supported interface
- Built-in session management
- Hook system for monitoring
- Handles all Claude Code features

**Cons**:
- Requires Node.js or Python runtime
- SDK spawns CLI internally (no direct process control)

**Implementation**:
```typescript
// TypeScript wrapper for Maestro
import { query, ClaudeAgentOptions } from '@anthropic-ai/claude-agent-sdk';

export class ClaudeAgent {
  private sessionId: string | null = null;

  async run(prompt: string, options: Partial<ClaudeAgentOptions>) {
    const result = query({
      prompt,
      options: {
        ...options,
        resume: this.sessionId ?? undefined,
        hooks: {
          PostToolUse: [{
            hooks: [async (input) => {
              // Emit to Maestro's event system
              this.emit('tool_used', input);
              return {};
            }]
          }]
        }
      }
    });

    for await (const message of result) {
      this.emit('message', message);
      if (message.type === 'result') {
        this.sessionId = message.session_id;
      }
    }
  }
}
```

### Option B: Wrap CLI in PTY (Like CodexMonitor)

**Approach**: Use `portable-pty` to spawn `claude` CLI directly.

**Pros**:
- Direct process control
- Works with Rust backend
- Can capture raw terminal output

**Cons**:
- No structured message parsing (must parse ANSI/text)
- Miss hook system benefits
- More complex to manage sessions
- Duplicates what SDK does internally

**Not Recommended** - The SDK already wraps the CLI; doing it again adds complexity without benefit.

### Option C: Custom Tool via MCP

**Approach**: Run Maestro as an MCP server that Claude Code connects to.

**Pros**:
- Clean separation of concerns
- MCP is a standard protocol

**Cons**:
- Inverts control (Claude calls Maestro, not vice versa)
- Doesn't solve agent orchestration
- Not suitable for our use case

### Option D: Headless CLI Wrapper

**Approach**: Run `claude -p` with `--output-format stream-json` and build a server wrapper around the CLI stream.

**Pros**:
- Matches CLI behavior and flags
- Simple to prototype

**Cons**:
- Still spawns a CLI process per session
- Less structured than SDK message types
- Harder to manage long-lived sessions vs SDK `resume`

### Comparison Table

| Aspect | SDK Direct | PTY Wrapper | MCP Tool | Headless CLI |
|--------|------------|-------------|----------|--------------|
| Message parsing | Structured | Manual | N/A | Semi-structured |
| Tool events | Hooks | None | N/A | CLI stream only |
| Session mgmt | Built-in | Manual | N/A | Manual |
| Rust integration | Via subprocess | Native | Native | Via subprocess |
| Complexity | Low | High | Medium | Medium |
| Official support | Yes | No | Partial | Yes |

---

## 6. Recommendations

### Primary Approach

**Use the TypeScript SDK** (`@anthropic-ai/claude-agent-sdk`) via Bun subprocess from Rust backend.

Rationale:
1. Official, supported API
2. Full hook system for monitoring tool execution
3. Structured message types (no parsing needed)
4. Session management built-in
5. All Claude Code features accessible

### Server-Oriented Design Notes

- Treat the daemon as a **session broker**: map remote client IDs to SDK `session_id` values and persist them for resume.
- Each session still requires a CLI subprocess under the hood; plan for process lifecycle, timeouts, and cleanup.
- Use SDK presets + `settingSources` to mirror CLI behavior when you need terminal-equivalent sessions.
- Expose operations over the server API: `start`, `resume`, `interrupt`, `setModel`, `setPermissionMode`, `rewindFiles`.
- Stream `SDKMessage` events over your wire protocol; keep result events as definitive session boundaries.

### Architecture Pattern

```
┌─────────────────────────────────────────────────────────────┐
│                        Maestro UI                           │
│                    (Tauri Frontend)                         │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     Maestro Backend                         │
│                      (Rust/Tauri)                           │
│  ┌─────────────────────────────────────────────────────┐   │
│  │              Agent Manager (Rust)                    │   │
│  │  - Spawns Bun subprocess per agent                   │   │
│  │  - Manages agent lifecycle                           │   │
│  │  - Routes messages to UI                             │   │
│  └─────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┼─────────────────────┐
        ▼                     ▼                     ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│ Bun Process   │   │ Bun Process   │   │ Bun Process   │
│ (Agent 1)     │   │ (Agent 2)     │   │ (Agent 3)     │
│ SDK wrapper   │   │ SDK wrapper   │   │ SDK wrapper   │
└───────────────┘   └───────────────┘   └───────────────┘
        │                     │                     │
        ▼                     ▼                     ▼
┌───────────────┐   ┌───────────────┐   ┌───────────────┐
│ Claude Code   │   │ Claude Code   │   │ Claude Code   │
│ CLI Process   │   │ CLI Process   │   │ CLI Process   │
└───────────────┘   └───────────────┘   └───────────────┘
```

### Key Implementation Details

1. **Bun Process Wrapper**:
   - Create thin TypeScript script that imports SDK
   - Communicates with Rust via stdin/stdout JSON
   - Emits structured events for tool usage, messages, results
   - Bun provides faster startup and native TypeScript support

2. **Event Bridge**:
   ```typescript
   // agent-wrapper.ts (runs with Bun)
   for await (const line of Bun.stdin.stream()) {
     const cmd = JSON.parse(new TextDecoder().decode(line));
     if (cmd.type === 'start') {
       await runAgent(cmd.prompt, cmd.options);
     } else if (cmd.type === 'interrupt') {
       abortController.abort();
     }
   }

   function emit(event: string, data: any) {
     console.log(JSON.stringify({ event, data }));
   }
   ```

3. **Rust Side**:
   ```rust
   // Spawn Bun process
   let child = Command::new("bun")
       .arg("run")
       .arg("agent-wrapper.ts")
       .stdin(Stdio::piped())
       .stdout(Stdio::piped())
       .spawn()?;

   // Send commands
   writeln!(child.stdin, "{}", serde_json::to_string(&cmd)?)?;

   // Read events
   for line in BufReader::new(child.stdout).lines() {
       let event: AgentEvent = serde_json::from_str(&line?)?;
       // Forward to UI
   }
   ```

### Gaps and Limitations

1. **No Server Mode**: Claude Code doesn't have `app-server` like Codex. Each agent is a separate process.

2. **No Remote Attachment**: Can't attach to running CLI sessions. Must spawn new sessions via SDK.

3. **Session Persistence**: Sessions are stored locally in `~/.claude/`. Resuming requires the session ID.

4. **MCP Serve Is Not a Session Server**: `claude mcp serve` only exposes Claude Code tools to MCP clients; it does not expose an interactive Claude Code session loop.

5. **SDK Auth**: SDK uses API keys (or Bedrock/Vertex/Foundry envs). Claude.ai OAuth login is CLI-only.

6. **Background Agents**: The SDK supports background agent spawning via the `Task` tool, but orchestration is within the agent loop.

7. **Cost Tracking**: Available via `ResultMessage.total_cost_usd` and per-model usage stats.

### Code Snippets

#### Start Agent and Stream Events

```typescript
import { query } from '@anthropic-ai/claude-agent-sdk';

async function* runAgent(prompt: string, cwd: string) {
  const controller = new AbortController();

  const stream = query({
    prompt,
    options: {
      cwd,
      abortController: controller,
      permissionMode: 'acceptEdits',
      allowedTools: ['Read', 'Write', 'Edit', 'Bash', 'Glob', 'Grep'],
      hooks: {
        PreToolUse: [{
          hooks: [async (input) => {
            yield { type: 'tool_start', tool: input.tool_name, input: input.tool_input };
            return {};
          }]
        }],
        PostToolUse: [{
          hooks: [async (input) => {
            yield { type: 'tool_end', tool: input.tool_name, result: input.tool_response };
            return {};
          }]
        }]
      }
    }
  });

  for await (const message of stream) {
    if (message.type === 'assistant') {
      for (const block of message.message.content) {
        if (block.type === 'text') {
          yield { type: 'text', text: block.text };
        }
      }
    } else if (message.type === 'result') {
      yield {
        type: 'complete',
        sessionId: message.session_id,
        cost: message.total_cost_usd,
        turns: message.num_turns
      };
    }
  }
}
```

#### Monitor Tool Usage

```typescript
const hooks = {
  PreToolUse: [{
    hooks: [async (input, toolUseId) => {
      // Log all tool invocations
      console.log(`[${toolUseId}] ${input.tool_name}:`, input.tool_input);

      // Block dangerous commands
      if (input.tool_name === 'Bash' && input.tool_input.command.includes('rm -rf')) {
        return {
          hookSpecificOutput: {
            hookEventName: 'PreToolUse',
            permissionDecision: 'deny',
            permissionDecisionReason: 'Dangerous command blocked'
          }
        };
      }
      return {};
    }]
  }]
};
```

#### Custom Permissions Handler

```typescript
const options = {
  canUseTool: async (toolName, input, { signal }) => {
    // Custom approval logic
    if (toolName === 'Write' && input.file_path.includes('config')) {
      return {
        behavior: 'deny',
        message: 'Cannot write to config files',
        interrupt: false
      };
    }
    return {
      behavior: 'allow',
      updatedInput: input
    };
  }
};
```

---

## 7. Questions Answered

| Question | Answer |
|----------|--------|
| Does Claude Code have `app-server` mode? | **No.** Each session is a process spawned by the SDK. |
| Does the SDK use Claude Code under the hood? | **Yes.** The SDK shells out to the `claude` CLI runtime. |
| Does updating Claude Code change SDK behavior? | **Yes.** SDK execution semantics follow the installed CLI version. |
| Is there a thread/message API? | **Yes.** `query()` returns async generator of typed messages. |
| How do we receive real-time output? | **Streaming.** Iterate the generator; optionally enable `includePartialMessages`. |
| Can we attach to running CLI sessions? | **No.** But can resume sessions by ID via `resume` option. |
| How do background agents work? | Via `Task` tool or `agents` config. SDK manages subprocess spawning. |

---

## 8. Next Steps

1. Create Bun wrapper script (`packages/agent-wrapper/`)
2. Define JSON protocol between Rust and Bun
3. Implement Rust `AgentManager` that spawns/manages wrappers
4. Add Tauri commands for agent lifecycle
5. Build UI components for agent output streaming

---

## References

- [Agent SDK Overview](https://platform.claude.com/docs/en/agent-sdk/overview)
- [TypeScript SDK Reference](https://platform.claude.com/docs/en/agent-sdk/typescript)
- [Python SDK Reference](https://platform.claude.com/docs/en/agent-sdk/python)
- [Hooks Guide](https://platform.claude.com/docs/en/agent-sdk/hooks)
- [SDK Examples](https://github.com/anthropics/claude-agent-sdk-demos)
- [GitHub: claude-code](https://github.com/anthropics/claude-code)
- [Claude Code CLI Reference](https://code.claude.com/docs/en/cli-reference)
- [Claude Code Settings](https://code.claude.com/docs/en/settings)
- [Claude Code Programmatic Usage](https://code.claude.com/docs/en/headless)
- [Claude Code MCP](https://code.claude.com/docs/en/mcp)
