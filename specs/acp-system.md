<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Agent Client Protocol (ACP) System

## Overview

The ACP system enables Loom to be driven by external clients (code editors like Zed, VSCode) via the
[Agent Client Protocol](https://agentclientprotocol.com/). When running in ACP mode, Loom acts as an
ACP Agent that communicates over stdio using JSON-RPC, allowing editors to send prompts and receive
streaming responses.

This integration allows Loom to be used as the AI backend for editor-based coding assistants while
reusing the existing agent state machine, tool system, LLM clients, and thread persistence.

### Architecture

```
┌─────────────────────┐          stdio           ┌─────────────────────┐
│   Editor (Client)   │◄────── JSON-RPC ────────►│  loom acp-agent     │
│   (Zed, VSCode)     │                          │                     │
│                     │  initialize              │  ┌───────────────┐  │
│  - Send prompts     │  session/new             │  │ LoomAcpAgent  │  │
│  - Receive streams  │  session/prompt          │  │ (acp::Agent)  │  │
│  - Show tool calls  │  session/update ◄────    │  └───────┬───────┘  │
└─────────────────────┘                          │          │          │
                                                 │          ▼          │
                                                 │  ┌───────────────┐  │
                                                 │  │  Loom Agent   │  │
                                                 │  │ State Machine │  │
                                                 │  └───────┬───────┘  │
                                                 │          │          │
                                                 │    ┌─────┴─────┐    │
                                                 │    ▼           ▼    │
                                                 │ ┌─────┐   ┌───────┐ │
                                                 │ │Tools│   │Thread │ │
                                                 │ │     │   │Store  │ │
                                                 │ └─────┘   └───────┘ │
                                                 └─────────────────────┘
```

## Crate Structure

### loom-acp

A new crate that bridges ACP protocol to Loom internals:

```
crates/loom-acp/
├── Cargo.toml
└── src/
    ├── lib.rs          # Public API, LoomAcpAgent
    ├── agent.rs        # acp::Agent trait implementation
    ├── session.rs      # Session state management
    ├── bridge.rs       # ACP ↔ Loom type conversions
    └── error.rs        # Error types and conversions
```

**Dependencies:**

- `agent-client-protocol` - ACP SDK
- `loom-core` - Agent state machine, LLM types
- `loom-thread` - Thread persistence
- `loom-tools` - Tool registry and execution
- `loom-llm-proxy` - LLM client
- `tokio`, `tracing`, `async-trait`

**Design constraint:** `loom-acp` does NOT depend on `loom-cli`, maintaining clean layering.

## Core Types

### LoomAcpAgent

The main type implementing the ACP `Agent` trait:

```rust
pub struct LoomAcpAgent {
	/// LLM client for completions
	llm_client: Arc<dyn LlmClient>,

	/// Tool registry for tool execution
	tools: Arc<ToolRegistry>,

	/// Thread persistence
	thread_store: Arc<dyn ThreadStore>,

	/// Default workspace root for new sessions
	default_workspace_root: PathBuf,

	/// Channel to send session notifications to client
	session_update_tx: mpsc::UnboundedSender<SessionNotificationRequest>,

	/// Active sessions (SessionId → SessionState)
	sessions: RefCell<HashMap<SessionId, SessionState>>,
}
```

### SessionState

Per-session state mapping ACP sessions to Loom threads:

```rust
struct SessionState {
	/// Corresponding Loom thread ID
	thread_id: ThreadId,

	/// Thread data (loaded/created)
	thread: Thread,

	/// Workspace root for this session
	workspace_root: PathBuf,

	/// Conversation messages for LLM requests
	messages: Vec<Message>,

	/// Cancellation flag
	cancelled: Arc<AtomicBool>,
}
```

## Protocol Mapping

### Session Lifecycle

| ACP Method       | Loom Action                                            |
| ---------------- | ------------------------------------------------------ |
| `initialize`     | Return agent info and capabilities                     |
| `authenticate`   | No-op (no auth required)                               |
| `session/new`    | Create new `Thread`, return `ThreadId` as `SessionId`  |
| `session/load`   | Load `Thread` from `ThreadStore`, rebuild conversation |
| `session/prompt` | Feed user input → run agent loop → stream responses    |
| `session/cancel` | Set cancellation flag, abort LLM stream                |

### Message Flow

```
Client                          LoomAcpAgent                    LLM
  │                                  │                           │
  │ ─── session/prompt ───────────►  │                           │
  │     { prompt: "Fix bug" }        │                           │
  │                                  │                           │
  │                                  │ ─── complete_streaming ──►│
  │                                  │                           │
  │ ◄── session/update ────────────  │ ◄─── TextDelta ──────────│
  │     AgentMessageChunk("I'll")    │                           │
  │                                  │                           │
  │ ◄── session/update ────────────  │ ◄─── TextDelta ──────────│
  │     AgentMessageChunk("fix")     │                           │
  │                                  │                           │
  │                                  │ ◄─── Completed ──────────│
  │                                  │      (with tool_calls)    │
  │                                  │                           │
  │ ◄── session/update ────────────  │                           │
  │     ToolCallStarted(edit_file)   │                           │
  │                                  │                           │
  │                                  │ ── execute tool locally ──│
  │                                  │                           │
  │ ◄── session/update ────────────  │                           │
  │     ToolCallFinished(success)    │                           │
  │                                  │                           │
  │                                  │ ─── complete_streaming ──►│
  │                                  │     (with tool results)   │
  │                                  │                           │
  │ ◄── session/update ────────────  │ ◄─── TextDelta ──────────│
  │     AgentMessageChunk("Done!")   │                           │
  │                                  │                           │
  │ ◄── PromptResponse ────────────  │ ◄─── Completed ──────────│
  │     { stop_reason: EndTurn }     │                           │
```

### Content Block Conversion

ACP uses `ContentBlock` for prompt content:

```rust
fn acp_content_to_loom_message(blocks: Vec<ContentBlock>) -> Message {
	let text = blocks
		.iter()
		.filter_map(|block| match block {
			ContentBlock::Text(t) => Some(t.text.as_str()),
			_ => None, // Images, audio, etc. not yet supported
		})
		.collect::<Vec<_>>()
		.join("\n");

	Message::user(&text)
}
```

### Stop Reason Mapping

| Loom State              | ACP StopReason |
| ----------------------- | -------------- |
| `WaitingForUserInput`   | `EndTurn`      |
| `ShuttingDown`          | `EndTurn`      |
| Cancellation flag set   | `Cancelled`    |
| Error during processing | `Error`        |

## Implementation Details

### Agent Trait Implementation

```rust
#[async_trait(?Send)]
impl acp::Agent for LoomAcpAgent {
	async fn initialize(&self, req: InitializeRequest) -> Result<InitializeResponse> {
		Ok(InitializeResponse {
			protocol_version: V1,
			agent_capabilities: AgentCapabilities {
				load_session: true,
				..Default::default()
			},
			auth_methods: Vec::new(),
			agent_info: Some(Implementation {
				name: "loom".into(),
				title: Some("Loom AI Coding Assistant".into()),
				version: env!("CARGO_PKG_VERSION").into(),
			}),
			meta: None,
		})
	}

	async fn new_session(&self, req: NewSessionRequest) -> Result<NewSessionResponse> {
		// 1. Create Thread
		// 2. Persist to ThreadStore
		// 3. Create SessionState
		// 4. Return SessionId (= ThreadId)
	}

	async fn load_session(&self, req: LoadSessionRequest) -> Result<LoadSessionResponse> {
		// 1. Parse SessionId as ThreadId
		// 2. Load Thread from ThreadStore
		// 3. Rebuild conversation messages
		// 4. Create SessionState
	}

	async fn prompt(&self, req: PromptRequest) -> Result<PromptResponse> {
		// 1. Get SessionState
		// 2. Convert ContentBlocks to Message
		// 3. Run agent loop:
		//    - Call LLM (streaming)
		//    - Stream TextDeltas as session/update
		//    - Execute tools
		//    - Send tool notifications
		//    - Repeat until WaitingForUserInput
		// 4. Persist Thread
		// 5. Return PromptResponse with stop_reason
	}

	async fn cancel(&self, req: CancelNotification) -> Result<()> {
		// Set cancelled flag on SessionState
		// LLM stream will check and abort
	}
}
```

### Prompt Loop

The core prompt handling loop:

```rust
async fn run_prompt_loop(&self, session: &mut SessionState) -> Result<StopReason> {
	loop {
		// Check cancellation
		if session.cancelled.load(Ordering::Relaxed) {
			return Ok(StopReason::Cancelled);
		}

		// Build LLM request
		let request = LlmRequest::new("default")
			.with_messages(session.messages.clone())
			.with_tools(self.tools.definitions());

		// Stream LLM response
		let mut stream = self.llm_client.complete_streaming(request).await?;
		let mut assistant_content = String::new();
		let mut tool_calls = Vec::new();

		while let Some(event) = stream.next().await {
			match event {
				LlmEvent::TextDelta { content } => {
					assistant_content.push_str(&content);
					self
						.send_message_chunk(&session.session_id, content)
						.await?;
				}
				LlmEvent::Completed(response) => {
					tool_calls = response.tool_calls;
					break;
				}
				LlmEvent::Error(e) => return Err(e.into()),
				_ => {}
			}
		}

		// Add assistant message to conversation
		session
			.messages
			.push(Message::assistant(&assistant_content));

		// If no tool calls, turn is complete
		if tool_calls.is_empty() {
			return Ok(StopReason::EndTurn);
		}

		// Execute tools
		for call in &tool_calls {
			self.send_tool_started(&session.session_id, call).await?;
			let outcome = self.execute_tool(call, &session.workspace_root).await;
			self
				.send_tool_finished(&session.session_id, call, &outcome)
				.await?;

			// Add tool result to conversation
			session
				.messages
				.push(Message::tool_result(&call.id, &outcome));
		}

		// Loop continues - LLM will process tool results
	}
}
```

### Session Notifications

Streaming content to the client:

```rust
async fn send_message_chunk(&self, session_id: &SessionId, text: String) -> Result<()> {
	let notification = SessionNotification {
		session_id: session_id.clone(),
		update: SessionUpdate::AgentMessageChunk(ContentChunk {
			content: ContentBlock::Text(TextBlock {
				text,
				..Default::default()
			}),
			meta: None,
		}),
		meta: None,
	};

	let (tx, rx) = oneshot::channel();
	self.session_update_tx.send((notification, tx))?;
	rx.await?;
	Ok(())
}
```

## CLI Integration

### New Subcommand

```rust
#[derive(Subcommand, Debug)]
enum Command {
	// ... existing commands ...
	/// Run as ACP agent over stdio (for editor integration)
	AcpAgent,
}
```

### Entry Point

```rust
async fn run_acp_agent(
	args: &Args,
	config: &LoomConfig,
	thread_store: Arc<dyn ThreadStore>,
) -> Result<()> {
	let llm_client = create_llm_client(&args.server_url, &args.provider)?;
	let tools = Arc::new(create_tool_registry());
	let workspace = config.workspace_root().canonicalize()?;

	let (tx, mut rx) = mpsc::unbounded_channel();
	let agent = LoomAcpAgent::new(llm_client, tools, thread_store, workspace, tx);

	let stdin = tokio::io::stdin().compat();
	let stdout = tokio::io::stdout().compat_write();

	let local_set = tokio::task::LocalSet::new();
	local_set
		.run_until(async move {
			let (conn, io_task) = AgentSideConnection::new(agent, stdout, stdin, |f| {
				tokio::task::spawn_local(f);
			});

			// Background task: send notifications to client
			tokio::task::spawn_local(async move {
				while let Some((notification, tx)) = rx.recv().await {
					if let Err(e) = conn.session_notification(notification).await {
						tracing::error!(error = %e, "failed to send session notification");
						break;
					}
					tx.send(()).ok();
				}
			});

			io_task.await
		})
		.await
}
```

## Thread Persistence

### Mapping to Existing System

- `SessionId` string == `ThreadId` string
- All thread metadata (git info, workspace, provider) populated as in REPL mode
- Messages persisted as `MessageSnapshot` after each assistant turn
- Tool calls persisted as `ToolCallSnapshot`
- Thread saved after each complete turn

### Session Recovery

On `load_session`:

1. Load `Thread` from `ThreadStore`
2. Convert `MessageSnapshot` list back to `Vec<Message>`
3. Resume from last state

## Design Decisions

### Why Local Tool Execution

Tools execute directly on the local filesystem (via `ToolRegistry`) rather than through ACP's
`fs.read_text_file` / `fs.write_text_file` callbacks because:

1. **Simplicity** - Reuses existing tool implementations unchanged
2. **Performance** - No round-trip through editor for file operations
3. **Consistency** - Same behavior in REPL and ACP modes
4. **Use case** - Loom always runs locally with direct filesystem access

If sandboxed execution is needed in the future, tools can be refactored to use an abstract
`FsClient` trait with `AcpFsClient` implementation.

### Why RefCell for Sessions

The ACP SDK's `Agent` trait is `?Send` (not thread-safe) because it's designed for single-threaded
async runtimes. Using `RefCell<HashMap<SessionId, SessionState>>` is safe because:

1. All access happens on the same `LocalSet`
2. We never hold borrows across `.await` points
3. This matches the pattern in the ACP example agent

### Why Separate Crate

`loom-acp` is separate from `loom-cli` to:

1. Keep ACP-specific dependencies isolated
2. Allow potential reuse in other binaries
3. Maintain clean dependency graph (loom-acp doesn't depend on loom-cli)

## Testing Strategy

### Unit Tests

- Session creation and lookup
- Content block conversion
- Stop reason mapping
- Error handling

### Integration Tests

```rust
#[tokio::test]
async fn test_prompt_round_trip() {
	// 1. Create mock LLM client that returns known response
	// 2. Create LoomAcpAgent with mock
	// 3. Call new_session
	// 4. Call prompt with test input
	// 5. Verify session notifications received
	// 6. Verify PromptResponse has correct stop_reason
}
```

### Property-Based Tests

```rust
proptest! {
		/// Session IDs are correctly round-tripped through ThreadId
		#[test]
		fn session_thread_id_roundtrip(id in "[a-zA-Z0-9-]{36}") {
				let session_id = SessionId(id.clone().into());
				let thread_id = ThreadId::from_string(id.clone());
				let back = SessionId(thread_id.to_string().into());
				assert_eq!(session_id.0.as_ref(), back.0.as_ref());
		}
}
```

## Future Enhancements

1. **Session modes** - Expose Loom modes (if any) via ACP `set_session_mode`
2. **Model selection** - Allow client to select models via `set_session_model`
3. **Progress reporting** - Stream tool progress via session notifications
4. **Cancellation** - More robust cancellation with cleanup
5. **MCP servers** - Honor `mcp_servers` in `NewSessionRequest`
