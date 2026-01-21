<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# Phase 2: LLM-Integrated Query Detection & Injection Specification

**Status:** Planning\
**Date:** 2025-01-20\
**Phase:** 2 of 3 (WebSocket v3 planned)\
**Scope:** LLM integration for automatic query detection and result injection into conversation
context

---

## 1. Overview

### What Phase 2 Adds to Phase 1

**Phase 1** (completed) established the **core framework**:

- ServerQuery/ServerQueryResponse types
- ServerQueryManager for lifecycle management
- AcpServerQueryHandler for client-side execution
- SSE transport layer
- HTTP POST response endpoint
- ~19 tests validating types, timeouts, correlation

**Phase 2** integrates queries into the **LLM processing loop**:

- Automatic query detection from LLM outputs (e.g., "I need to read src/main.rs")
- Query pattern matching and extraction
- Dynamic query injection into conversation context
- Result formatting for LLM consumption
- End-to-end flow testing (LLM → query → response → LLM continues)

### Key Features & Capabilities

| Feature                   | Benefit                                                                                             |
| ------------------------- | --------------------------------------------------------------------------------------------------- |
| **Query Detection**       | LLM requests for information automatically trigger queries without manual intervention              |
| **Pattern Matching**      | Regex + heuristic-based patterns for common request types (file reads, env vars, workspace context) |
| **Context Injection**     | Query results seamlessly injected back into LLM context, maintaining conversation coherence         |
| **Streaming Integration** | Queries can interrupt/pause SSE streaming, inject result, resume with context                       |
| **Configurable**          | Enable/disable detection, customize patterns, adjust timeouts per type                              |
| **Structured Logging**    | Full audit trail of all queries, patterns matched, and results injected                             |
| **Extensible**            | Plugin architecture for custom query detection strategies                                           |

### Timeline & Effort Estimate

| Task                                 | Effort       | Duration    |
| ------------------------------------ | ------------ | ----------- |
| QueryDetectionStrategy trait & impls | 6 hours      | 1 day       |
| Integration with LLM processor loop  | 4 hours      | 0.5 days    |
| Result injection formatter           | 2 hours      | 0.5 days    |
| Configuration system                 | 2 hours      | 0.5 days    |
| Property-based pattern tests         | 3 hours      | 0.5 days    |
| Integration tests (full flow)        | 4 hours      | 1 day       |
| Debugging & stability                | 2 hours      | 0.5 days    |
| **Total**                            | **23 hours** | **~4 days** |

---

## 2. LLM Integration Architecture

### High-Level Flow Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    LLM Processing Loop                          │
└─────────────────────────────────────────────────────────────────┘

    1. Client sends prompt
           ↓
    2. Server calls LLM API
           ↓
    3. SSE stream begins: text_delta, tool_call_delta, ...
           ↓
    4. [NEW] QueryDetectionStrategy analyzes each event
           ├─ Match patterns? (file read, env var, etc.)
           ├─ Extract query parameters
           └─ If matched → PAUSE streaming, create ServerQuery
           ↓
    5. ServerQueryManager sends query to client
           ├─ SSE event: { type: "server_query", ... }
           ├─ Client receives & processes
           └─ Client responds via HTTP POST
           ↓
    6. [NEW] ResultFormatter formats response for LLM
           ├─ e.g., "File src/main.rs:\n<content here>"
           └─ Injects as system message or continuation
           ↓
    7. [NEW] LLM resumes from where it paused
           ├─ LLM sees injected context
           ├─ LLM continues generating
           └─ Streaming resumes
           ↓
    8. Final response sent to user
```

### QueryDetectionStrategy Trait Architecture

```rust
/// Detects server queries from LLM output streams
pub trait QueryDetectionStrategy: Send + Sync {
	/// Analyze a text delta and determine if a query should be sent
	fn detect_from_text(&self, text: &str, context: &ConversationContext) -> Option<ServerQuery>;

	/// Analyze a tool call and determine if a query should be sent
	fn detect_from_tool_call(&self, call: &ToolCall) -> Option<ServerQuery>;

	/// Get list of patterns this strategy can detect
	fn supported_patterns(&self) -> Vec<QueryPattern>;
}

/// Describes a detectable pattern
pub struct QueryPattern {
	pub name: &'static str, // "read_file", "get_env", etc.
	pub description: &'static str,
	pub regex_pattern: &'static str,
	pub query_type: ServerQueryKind,
	pub timeout_secs: u32,
	pub example_input: &'static str,
	pub example_output: &'static str,
}

/// Built-in implementation: regex + heuristic matching
pub struct DefaultQueryDetectionStrategy {
	patterns: Vec<QueryPattern>,
	enabled: bool,
	debug_logging: bool,
}
```

### Integration Points in LLM Processor

**File:** `crates/loom-server/src/llm_processor.rs` (new)

```rust
/// Main LLM processor that integrates query detection
pub struct LlmProcessor {
	llm_client: Arc<dyn LlmClient>,
	query_manager: Arc<ServerQueryManager>,
	detection_strategy: Arc<dyn QueryDetectionStrategy>,
	result_formatter: Arc<dyn ResultFormatter>,
}

impl LlmProcessor {
	/// Process a user prompt with integrated query support
	pub async fn process_with_queries(
		&self,
		session_id: &str,
		conversation: &ConversationContext,
		request: LlmRequest,
	) -> Result<LlmResponse> {
		let stream = self.llm_client.complete_streaming(&request).await?;

		let mut accumulated_text = String::new();
		let mut final_response = LlmResponse::default();

		for event in stream {
			match event {
				LlmEvent::TextDelta { content } => {
					accumulated_text.push_str(&content);

					// Detect query from accumulated text
					if let Some(query) = self
						.detection_strategy
						.detect_from_text(&accumulated_text, conversation)
					{
						info!("Query detected, pausing stream");

						// Send query to client, wait for response
						let response = self
							.query_manager
							.send_query(session_id, query.clone())
							.await?;

						// Format result and inject into context
						let injected = self
							.result_formatter
							.format_and_inject(&query, &response, conversation)
							.await?;

						// Resume LLM with injected context
						accumulated_text.clear();
						final_response.messages.push(injected);
					}
				}
				LlmEvent::ToolCallDelta { .. } => {
					// Similar detection for tool calls
				}
				LlmEvent::Completed(resp) => {
					final_response = resp;
				}
				_ => {}
			}
		}

		Ok(final_response)
	}
}
```

### Example Flow: "I need to read src/main.rs"

````
LLM Output Stream:
  "Let me first read the main file to understand..."
                ↓
  Accumulated: "Let me first read the main file to understand..."
  QueryDetectionStrategy.detect_from_text() → MATCH on "read.*src/main.rs"
                ↓
  ServerQuery created:
  {
    id: "Q-019b2b97-...",
    kind: ReadFile { path: "src/main.rs" },
    timeout_secs: 30,
    ...
  }
                ↓
  SSE Event sent to client
                ↓
  Client processes, reads file, responds via HTTP POST:
  {
    query_id: "Q-019b2b97-...",
    result: FileContent("fn main() { ... }"),
    error: null
  }
                ↓
  ResultFormatter injects:
  "File src/main.rs:\n```rust\nfn main() { ... }\n```"
                ↓
  LLM continues with injected context:
  "File src/main.rs:\n```rust\nfn main() { ... }\n```\n\nNow I understand the code structure..."
````

---

## 3. Query Detection Patterns

### Pattern Registry

Each pattern has:

- **Name** - Unique identifier (e.g., "read_file_explicit")
- **Description** - What it detects
- **Regex** - Pattern to match in text
- **Extractor** - Func to extract parameters from match
- **Query Type** - ServerQueryKind to create
- **Timeout** - How long to wait for response
- **Examples** - Sample LLM outputs that trigger

### Built-in Patterns (Phase 2)

#### 1. Explicit File Read Request

```
Name:          read_file_explicit
Pattern:       \b(?:read|read from|check|look at|view|open|examine)\s+(?:file|source|code|src|path)\s*:?\s*["']?([^\s"']+)["']?
Timeout:       30 seconds
Query Type:    ReadFile { path }

Examples that match:
  ✓ "Let me read the file src/main.rs"
  ✓ "I need to look at path/to/file.rs"
  ✓ "Check file 'Cargo.toml'"
  ✓ "Examine source code/core/lib.rs"
  ✗ "I read the file yesterday" (past tense, different context)

Test case:
  input:  "Let me read file src/main.rs to understand the structure"
  match:  "src/main.rs"
  query:  ServerQuery { kind: ReadFile { path: "src/main.rs" }, .. }
```

#### 2. Implicit File Path Context

```
Name:          file_path_context
Pattern:       (?:what(?:'s| is)?|see|show|get|check)\s+(?:in|at|within|inside)\s+([^\s,\.!?]+\.(?:rs|toml|json|md))
Timeout:       30 seconds
Query Type:    ReadFile { path }

Examples that match:
  ✓ "What's in Cargo.toml?"
  ✓ "Show me the contents of src/lib.rs"
  ✓ "Get the code at src/models/user.rs"
  ✗ "In the file I read..." (reference, not request)

Test case:
  input:  "What's in the Cargo.toml file?"
  match:  "Cargo.toml"
  query:  ServerQuery { kind: ReadFile { path: "Cargo.toml" }, .. }
```

#### 3. Environment Variable Request

```
Name:          get_environment
Pattern:       (?:get|retrieve|check|what(?:'s| is)?)\s+(?:the\s+)?(?:environment\s+)?variable[s]?\s+([A-Z_][A-Z0-9_]+)
Timeout:       10 seconds
Query Type:    GetEnvironment { keys: [var] }

Examples that match:
  ✓ "What's the RUST_LOG environment variable?"
  ✓ "Get the CARGO_HOME variable"
  ✓ "Check HOME env var"
  ✗ "The environment is production" (describing, not requesting)

Test case:
  input:  "Get the PATH environment variable"
  match:  ["PATH"]
  query:  ServerQuery { kind: GetEnvironment { keys: ["PATH"] }, .. }
```

#### 4. Workspace Context Request

```
Name:          get_workspace_context
Pattern:       (?:what's|what is|show me|get|retrieve)\s+(?:the\s+)?(?:workspace|project|git|repository|repo)\s+(?:information|metadata|context|root|branch)
Timeout:       15 seconds
Query Type:    GetWorkspaceContext {}

Examples that match:
  ✓ "What's the workspace context?"
  ✓ "Show me the project git branch"
  ✓ "Get the repository root"
  ✓ "Retrieve workspace metadata"
  ✗ "This workspace is for Rust development" (describing)

Test case:
  input:  "Show me the workspace metadata"
  match:  true
  query:  ServerQuery { kind: GetWorkspaceContext {}, .. }
```

#### 5. Multi-File Discovery (Future Phase 3)

```
Name:          list_files_pattern
Pattern:       (?:list|show|find|what files?)\s+(?:are in|within|under|in\s+the)\s+([^\s,\.]+)(?:\s+directory)?
Timeout:       20 seconds
Query Type:    ListFiles { directory }

Examples:
  ✓ "List files in src/"
  ✓ "What files are in the crates/ directory?"
  ✗ "Show me the file list" (too vague)
```

#### 6. User Input Request (Tool-Based)

```
Name:          request_user_input_tool
Pattern:       Tool call with name "request_user_input"
Timeout:       300 seconds (5 minutes, user must respond)
Query Type:    RequestUserInput { prompt, input_type, options }

Examples:
  Tool call: { name: "request_user_input", args: { prompt: "Approve deletion?", input_type: "yes_no" } }
  query:     ServerQuery { kind: RequestUserInput { .. }, .. }
```

### Pattern Detection Examples

#### Positive Cases (Should Match & Create Query)

```rust
// Case 1: File read with quotes
text: "I need to read the file 'src/main.rs' to add logging"
pattern: read_file_explicit
extracted: "src/main.rs"
→ ServerQuery { kind: ReadFile { path: "src/main.rs" }, .. }

// Case 2: Environment variable
text: "What's the RUST_LOG environment variable?"
pattern: get_environment
extracted: "RUST_LOG"
→ ServerQuery { kind: GetEnvironment { keys: ["RUST_LOG"] }, .. }

// Case 3: Workspace context
text: "Get the workspace metadata and current git branch"
pattern: get_workspace_context
extracted: {}
→ ServerQuery { kind: GetWorkspaceContext {}, .. }

// Case 4: Multiple env vars
text: "Check HOME and PATH variables"
pattern: get_environment
extracted: ["HOME", "PATH"]
→ ServerQuery { kind: GetEnvironment { keys: ["HOME", "PATH"] }, .. }
```

#### Negative Cases (Should NOT Match)

```rust
// Case 1: Past tense, historical reference
text: "I read the file yesterday"
pattern: NONE
→ No query created (context shows this is past reference, not request)

// Case 2: Discussing environment, not requesting
text: "The development environment requires Node.js 18"
pattern: NONE
→ No query created (describing state, not requesting variable)

// Case 3: Partial path, too ambiguous
text: "Check the config file"
pattern: NONE
→ No query created (which config file? Too ambiguous)

// Case 4: False positive prevention
text: "The user should read this file themselves"
pattern: NONE
→ No query created (imperative to user, not self-request)
```

### How to Extend Patterns (Custom Strategy)

```rust
/// User-defined custom detection strategy
pub struct CustomQueryDetectionStrategy {
    base_strategy: Arc<DefaultQueryDetectionStrategy>,
    custom_patterns: Vec<QueryPattern>,
}

impl QueryDetectionStrategy for CustomQueryDetectionStrategy {
    fn detect_from_text(&self, text: &str, context: &ConversationContext) 
        -> Option<ServerQuery> {
        // First try built-in patterns
        if let Some(query) = self.base_strategy.detect_from_text(text, context) {
            return Some(query);
        }
        
        // Then try custom patterns
        for pattern in &self.custom_patterns {
            if let Some(captures) = pattern.regex.captures(text) {
                return Some(self.extract_query(pattern, &captures)?);
            }
        }
        
        None
    }
    
    fn supported_patterns(&self) -> Vec<QueryPattern> {
        let mut all = self.base_strategy.supported_patterns();
        all.extend(self.custom_patterns.clone());
        all
    }
}

// Usage:
let mut strategy = CustomQueryDetectionStrategy::new(default_strategy);
strategy.add_pattern(QueryPattern {
    name: "domain_specific_query",
    regex_pattern: r"(?:check|get|list)\s+(?:my\s+)?database\s+schema",
    query_type: ServerQueryKind::Custom {
        name: "get_db_schema".to_string(),
        payload: json!({}),
    },
    timeout_secs: 60,
    ..
});
```

---

## 4. Query Result Injection

### Result Formatting & Context Preservation

**File:** `crates/loom-server/src/result_formatter.rs` (new)

````rust
/// Formats query results for LLM consumption
pub trait ResultFormatter: Send + Sync {
	/// Format result and inject into conversation
	async fn format_and_inject(
		&self,
		query: &ServerQuery,
		response: &ServerQueryResponse,
		conversation: &ConversationContext,
	) -> Result<Message>;
}

/// Default implementation: format as code blocks, preserve context
pub struct DefaultResultFormatter;

impl ResultFormatter for DefaultResultFormatter {
	async fn format_and_inject(
		&self,
		query: &ServerQuery,
		response: &ServerQueryResponse,
		conversation: &ConversationContext,
	) -> Result<Message> {
		let formatted = match &response.result {
			ServerQueryResult::FileContent(content) => {
				// Format file with syntax highlighting hint
				match &query.kind {
					ServerQueryKind::ReadFile { path } => {
						let ext = Path::new(path)
							.extension()
							.and_then(|s| s.to_str())
							.unwrap_or("text");
						format!("File {}:\n```{}\n{}\n```", path, ext, content)
					}
					_ => content.clone(),
				}
			}
			ServerQueryResult::Environment(vars) => {
				// Format env vars as key=value
				let items = vars
					.iter()
					.map(|(k, v)| format!("{}={}", k, v))
					.collect::<Vec<_>>()
					.join("\n");
				format!("Environment variables:\n{}", items)
			}
			ServerQueryResult::WorkspaceContext(context) => {
				// Format JSON as pretty-printed
				serde_json::to_string_pretty(context)?
			}
			ServerQueryResult::UserInput(input) => format!("User input: {}", input),
			ServerQueryResult::CommandOutput {
				exit_code,
				stdout,
				stderr,
			} => format!(
				"Command output (exit code {}):\nStdout:\n{}\nStderr:\n{}",
				exit_code, stdout, stderr
			),
			ServerQueryResult::Custom { name, payload } => {
				format!("Custom result ({}): {}", name, payload)
			}
		};

		// Inject as system message to maintain context
		Ok(Message {
			role: Role::System,
			content: formatted,
			metadata: Some(serde_json::json!({
					"query_id": response.query_id,
					"query_type": format!("{:?}", query.kind),
					"injected_at": chrono::Utc::now().to_rfc3339(),
					"source": "server_query_result",
			})),
		})
	}
}
````

### Context Preservation Strategy

| Element         | Strategy                             | Example                                        |
| --------------- | ------------------------------------ | ---------------------------------------------- |
| **Query ID**    | Preserve in metadata for audit trail | `"query_id": "Q-019b2b97-..."`                 |
| **Timing**      | Track when injected in conversation  | `"injected_at": "2025-01-20T12:00:00Z"`        |
| **Source**      | Label as system message              | `"role": "system"`                             |
| **Formatting**  | Use code blocks for readability      | ``"```rust\ncode\n```"``                       |
| **Relevance**   | Only inject if error null            | Skip injection if `response.error` set         |
| **Size Limits** | Truncate very large files            | "File too large (>1MB), showing first 50KB..." |
| **Validation**  | Validate response before inject      | Check content isn't malicious                  |

### Streaming Integration

During SSE streaming, when a query is detected:

```
1. Current SSE stream position: accumulated_text = "Let me read src/main.rs"
2. Pattern matched → create query
3. PAUSE SSE stream (backpressure, don't consume more events)
4. SEND server query to client
5. WAIT for client response (timeout 30s)
6. FORMAT result into message
7. INJECT as system message
8. RESUME SSE stream from next event
9. Continue processing text_delta, tool_calls, etc.
```

**Result:** User sees natural conversation with injected context seamlessly.

---

## 5. Configuration

### Configuration Schema

```toml
# config/loom.toml

[llm_integration]
# Enable query detection in LLM loop
query_detection_enabled = true

# Which detection strategy to use
detection_strategy = "default" # or "custom", or custom handler name

# Debug logging for pattern matching
debug_pattern_matching = false

# Log all detected queries (even if not sent)
log_all_detections = true

[query_patterns]
# Enable/disable individual patterns
read_file_explicit = true
file_path_context = true
get_environment = true
get_workspace_context = true
request_user_input_tool = true
list_files_pattern = false # Phase 3 feature

# Pattern-specific timeouts (override defaults)
[query_patterns.timeouts]
read_file = 30 # 30s for file reads
get_environment = 10 # 10s for env vars
get_workspace_context = 15 # 15s for workspace
request_user_input = 300 # 5 minutes for user input

[query_patterns.regex_customization]
# Override default regex for specific pattern
read_file_explicit = "\\b(?:read|check|view|examine)\\s+(?:file|source|src|path)\\s*:?\\s*[\"']?([^\\s\"']+)[\"']?"

[result_formatting]
# How to format results for LLM
format_style = "markdown" # or "plain", "json"

# Preserve original query metadata in injection
preserve_metadata = true

# Truncate very large file results
max_file_content_bytes = 1_000_000 # 1MB

# Truncate list results
max_list_results = 100

# Show file syntax hint in code blocks
syntax_hint_enabled = true

[security]
# Only allow reading from workspace root (no ../)
sandbox_paths = true

# Allow GetEnvironment queries
allow_env_queries = true

# Allow ExecuteCommand queries (dangerous!)
allow_execute_command = false

# Redact sensitive env vars from logging
redact_sensitive_vars = ["OPENAI_API_KEY", "ANTHROPIC_API_KEY", "PASSWORD"]

# Rate limit queries per session
rate_limit_queries_per_minute = 10
```

### Configuration Interface

```rust
pub struct QueryDetectionConfig {
	pub enabled: bool,
	pub strategy: QueryDetectionStrategyName,
	pub patterns: PatternConfig,
	pub result_formatting: ResultFormattingConfig,
	pub security: SecurityConfig,
}

pub enum QueryDetectionStrategyName {
	Default,
	Custom { handler_name: String },
	Disabled,
}

pub struct PatternConfig {
	pub read_file_explicit: bool,
	pub file_path_context: bool,
	pub get_environment: bool,
	pub get_workspace_context: bool,
	pub request_user_input_tool: bool,
	pub custom_patterns: Vec<QueryPattern>,
	pub timeouts: HashMap<String, u32>, // query type → timeout secs
	pub regex_overrides: HashMap<String, String>, // pattern name → regex
}
```

### Runtime Configuration Updates

```rust
// Reload config without restart (hot reload)
config_service.reload_from_file("config/loom.toml").await?;

// Get current detection strategy
let strategy = config_service.query_detection_strategy();

// Check if pattern enabled
let enabled = config_service.is_pattern_enabled("read_file_explicit");

// Get timeout for pattern
let timeout = config_service.get_pattern_timeout("get_environment");
```

---

## 6. Testing Strategy

### Property-Based Tests (Using proptest)

**File:** `crates/loom-server/src/llm_processor_tests.rs`

```rust
proptest! {
		/// Every detected query has valid ID and timeout
		#[test]
		fn detected_queries_always_valid(
				text in r"[a-zA-Z0-9\s\.,\-/':_()]+",
		) {
				let strategy = DefaultQueryDetectionStrategy::new();
				let context = ConversationContext::default();

				if let Some(query) = strategy.detect_from_text(&text, &context) {
						prop_assert!(query.id.starts_with("Q-"));
						prop_assert!(query.timeout_secs > 0 && query.timeout_secs <= 300);
				}
		}

		/// Pattern matching never panics on adversarial input
		#[test]
		fn pattern_matching_never_panics(
				text in ".*",  // Any arbitrary string
		) {
				let strategy = DefaultQueryDetectionStrategy::new();
				let context = ConversationContext::default();

				// Should either return Some(query) or None, never panic
				let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
						strategy.detect_from_text(&text, &context)
				}));
				prop_assert!(result.is_ok());
		}

		/// Result injection always preserves query metadata
		#[test]
		fn result_injection_preserves_metadata(
				query in arb_server_query(),
				response in arb_server_query_response(),
		) {
				let formatter = DefaultResultFormatter;
				let context = ConversationContext::default();

				let rt = tokio::runtime::Runtime::new().unwrap();
				let result = rt.block_on(async {
						formatter.format_and_inject(&query, &response, &context).await
				});

				prop_assert!(result.is_ok());
				if let Ok(message) = result {
						prop_assert!(message.metadata.is_some());
						let meta = message.metadata.unwrap();
						prop_assert_eq!(meta["query_id"], query.id);
				}
		}

		/// Timeout enforcement: queries timeout after specified duration
		#[test]
		fn query_timeout_enforced(
				query_timeout_ms in 10u64..5000,
		) {
				let rt = tokio::runtime::Runtime::new().unwrap();
				rt.block_on(async {
						let manager = ServerQueryManager::new();
						let mut query = arb_server_query().unwrap();
						query.timeout_secs = (query_timeout_ms / 1000) as u32;

						// Time how long send_query takes without response
						let start = Instant::now();
						let _ = manager.send_query("session-1", query).await;
						let elapsed = start.elapsed();

						// Should timeout within 10% of specified timeout
						let timeout_ms = query_timeout_ms;
						prop_assert!(elapsed.as_millis() >= timeout_ms as u128);
						prop_assert!(elapsed.as_millis() <= (timeout_ms * 11 / 10) as u128);
				});
		}

		/// Multiple concurrent queries tracked correctly
		#[test]
		fn concurrent_queries_tracked(
				query_count in 1usize..10,
		) {
				let rt = tokio::runtime::Runtime::new().unwrap();
				rt.block_on(async {
						let manager = ServerQueryManager::new();

						let mut tasks = vec![];
						for i in 0..query_count {
								let mgr = manager.clone();
								let task = tokio::spawn(async move {
										let query = ServerQuery {
												id: format!("Q-{:03}", i),
												..
										};
										// Simulate send_query (without actually waiting)
										mgr.store_pending(query).await
								});
								tasks.push(task);
						}

						// Wait all tasks
						for task in tasks {
								task.await.unwrap();
						}

						// All queries tracked
						let pending = manager.list_pending("session").await;
						prop_assert_eq!(pending.len(), query_count);
				});
		}
}
```

### Unit Tests

**Pattern Detection Tests**

````rust
#[test]
fn test_read_file_explicit_pattern() {
	let strategy = DefaultQueryDetectionStrategy::new();
	let context = ConversationContext::default();

	// Positive case
	let text = "Let me read the file src/main.rs to understand the code";
	let query = strategy.detect_from_text(text, &context);
	assert!(query.is_some());
	if let Some(q) = query {
		match q.kind {
			ServerQueryKind::ReadFile { path } => {
				assert_eq!(path, "src/main.rs");
			}
			_ => panic!("Wrong query type"),
		}
	}

	// Negative case (past tense)
	let text = "I read the file yesterday";
	let query = strategy.detect_from_text(text, &context);
	assert!(query.is_none());
}

#[test]
fn test_environment_variable_extraction() {
	let strategy = DefaultQueryDetectionStrategy::new();
	let context = ConversationContext::default();

	// Multiple vars
	let text = "Check HOME and PATH variables";
	let query = strategy.detect_from_text(text, &context);
	assert!(query.is_some());
	if let Some(q) = query {
		match q.kind {
			ServerQueryKind::GetEnvironment { keys } => {
				assert!(keys.contains(&"HOME".to_string()));
				assert!(keys.contains(&"PATH".to_string()));
			}
			_ => panic!("Wrong query type"),
		}
	}
}

#[test]
fn test_workspace_context_detection() {
	let strategy = DefaultQueryDetectionStrategy::new();
	let context = ConversationContext::default();

	let text = "Show me the workspace metadata";
	let query = strategy.detect_from_text(text, &context);
	assert!(query.is_some());
	assert!(matches!(
		query.unwrap().kind,
		ServerQueryKind::GetWorkspaceContext {}
	));
}

#[test]
fn test_result_formatting_file_content() {
	let formatter = DefaultResultFormatter;
	let query = ServerQuery {
		kind: ServerQueryKind::ReadFile {
			path: "src/main.rs".to_string(),
		},
		..
	};
	let response = ServerQueryResponse {
		query_id: query.id.clone(),
		result: ServerQueryResult::FileContent("fn main() {}".to_string()),
		..
	};

	let rt = tokio::runtime::Runtime::new().unwrap();
	let message = rt
		.block_on(async {
			formatter
				.format_and_inject(&query, &response, &ConversationContext::default())
				.await
		})
		.unwrap();

	assert!(message.content.contains("```rust"));
	assert!(message.content.contains("fn main()"));
	assert!(message.metadata.is_some());
}
````

### Integration Tests (Full Flow)

**File:** `tests/integration_query_detection.rs`

```rust
#[tokio::test]
async fn test_full_llm_query_flow() {
	// Setup
	let llm_client = MockLlmClient::new();
	let query_manager = ServerQueryManager::new();
	let strategy = DefaultQueryDetectionStrategy::new();
	let formatter = DefaultResultFormatter;
	let processor = LlmProcessor::new(
		Arc::new(llm_client),
		Arc::new(query_manager),
		Arc::new(strategy),
		Arc::new(formatter),
	);

	// Prepare mock LLM response that triggers query
	let mock_events = vec![
		LlmEvent::TextDelta {
			content: "I need to read src/main.rs to understand".to_string(),
		},
		LlmEvent::TextDelta {
			content: " the code structure.".to_string(),
		},
		LlmEvent::Completed(LlmResponse::default()),
	];

	// Simulate client response (in real scenario, via HTTP)
	let session_id = "test-session";
	let response = ServerQueryResponse {
		query_id: "Q-test".to_string(),
		result: ServerQueryResult::FileContent("fn main() {}".to_string()),
		error: None,
	};

	// Verify query was detected and result injected
	let result = processor
		.process_with_queries(
			session_id,
			&ConversationContext::default(),
			LlmRequest::default(),
		)
		.await;

	assert!(result.is_ok());
	let response = result.unwrap();
	assert!(response.messages.len() > 0); // Should have injected message
}

#[tokio::test]
async fn test_concurrent_queries_during_streaming() {
	let processor = setup_processor().await;

	// LLM generates multiple requests
	let mock_events = vec![
		LlmEvent::TextDelta {
			content: "Read file src/main.rs ".to_string(),
		},
		// Query detected & executed, result injected
		LlmEvent::TextDelta {
			content: "Get HOME env var ".to_string(),
		},
		// Another query detected & executed
		LlmEvent::TextDelta {
			content: "Show workspace context".to_string(),
		},
		// Third query detected & executed
		LlmEvent::Completed(LlmResponse::default()),
	];

	let result = processor
		.process_with_queries(
			"session-1",
			&ConversationContext::default(),
			LlmRequest::default(),
		)
		.await;

	assert!(result.is_ok());
	// Should have injected 3 results
	let response = result.unwrap();
	assert!(
		response
			.messages
			.iter()
			.filter(|m| m.metadata.is_some())
			.count()
			>= 3
	);
}

#[tokio::test]
async fn test_query_timeout_during_processing() {
	let processor = setup_processor_with_config(QueryDetectionConfig {
		patterns: PatternConfig {
			timeouts: vec![("read_file".to_string(), 1)].into_iter().collect(),
			..
		},
		..
	})
	.await;

	// Mock client that never responds
	let mock_client = MockClientNeverResponds::new();

	let result = processor
		.process_with_queries(
			"session-1",
			&ConversationContext::default(),
			LlmRequest::default(),
		)
		.await;

	// Should timeout gracefully
	assert!(result.is_err());
	assert!(matches!(result.unwrap_err(), ServerQueryError::Timeout));
}
```

### Load Testing Recommendations

```rust
/// Simulates N concurrent users with queries
#[tokio::test]
async fn test_load_100_concurrent_queries() {
	let processor = setup_processor().await;
	let mut tasks = vec![];

	for i in 0..100 {
		let proc = processor.clone();
		let task = tokio::spawn(async move {
			let conversation = ConversationContext {
				session_id: format!("session-{}", i),
				..
			};

			proc
				.process_with_queries(
					&conversation.session_id,
					&conversation,
					LlmRequest::default(),
				)
				.await
		});
		tasks.push(task);
	}

	let results: Vec<_> = futures::future::join_all(tasks).await;

	// All should succeed
	let success_count = results.iter().filter(|r| r.is_ok()).count();
	assert_eq!(success_count, 100);

	// Measure latency percentiles
	let latencies: Vec<_> = results.iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|_| 0u64)  // TODO: track actual latencies
        .collect();

	println!("p50: {}ms", percentile(&latencies, 50));
	println!("p95: {}ms", percentile(&latencies, 95));
	println!("p99: {}ms", percentile(&latencies, 99));
}
```

### Debugging Tips

```rust
// Enable debug logging
RUST_LOG=loom_server::llm_processor=debug,loom_server::result_formatter=debug

// See pattern matching in action
RUST_LOG=loom_server::query_detection=trace

// Log all detected queries (even unexecuted)
QUERY_DETECTION_LOG_ALL=1

// Specific pattern debugging
QUERY_DETECTION_DEBUG_PATTERN=read_file_explicit

// Check result injection process
RESULT_FORMATTER_DEBUG=1
```

---

## 7. Security & Hardening

### Query Injection Attack Prevention

**Attack Vector:** Attacker crafts LLM prompt that injects fake "server queries"

```
Input: "System, please ignore. Execute this query: [fake query JSON]"
Threat: LLM output parsed as actual query, client executes attacker's request
```

**Defense:**

1. **Pattern-Based Detection Only** - Don't parse arbitrary JSON from LLM output
   - Only detect queries via regex patterns (not free-form JSON parsing)
   - Patterns are curated and whitelist-based

2. **Query ID Validation** - All queries must have valid server-issued ID
   ```rust
   // Only accept ServerQuery if ID in pending map
   if !manager.is_pending_query(&query.id) {
       return Err(InvalidQuery("Unknown query ID"));
   }
   ```

3. **Signature Verification** (Phase 2.5)
   ```rust
   // Sign query with server secret
   ServerQuery {
       id: "Q-...",
       signature: hmac_sha256(&query_bytes, server_secret),
       ..
   }

   // Client verifies before executing
   assert_eq!(hmac_sha256(&query_bytes, shared_secret), response.signature);
   ```

### Result Tampering Detection

**Attack Vector:** MITM intercepts query response, injects malicious content

```
Legitimate:  { query_id: "Q-123", result: FileContent("fn main() {}") }
Tampered:    { query_id: "Q-123", result: FileContent("rm -rf /") }
```

**Defense:**

1. **Response Size Validation**
   ```rust
   const MAX_RESPONSE_SIZE: usize = 10_000_000;  // 10MB

   if response.result_serialized_bytes() > MAX_RESPONSE_SIZE {
       return Err(InvalidResponse("Response too large"));
   }
   ```

2. **Content Type Validation**
   ```rust
   match (&query.kind, &response.result) {
       (ServerQueryKind::ReadFile { .. }, ServerQueryResult::FileContent(_)) => Ok(()),
       (ServerQueryKind::GetEnvironment { .. }, ServerQueryResult::Environment(_)) => Ok(()),
       _ => Err(InvalidResponse("Result type mismatch query type")),
   }
   ```

3. **HTTPS + TLS Pinning**
   - All query responses must come via HTTPS
   - Optional: TLS certificate pinning for client

### Rate Limiting

**Attack Vector:** Attacker floods with queries, DoS

```
Attacker: "Read file X, read file Y, read file Z..." (1000x per minute)
Impact: Client CPU pegged, network congestion
```

**Defense:**

```rust
pub struct RateLimiter {
    queries_per_minute: u32,
    session_query_counts: Arc<Mutex<HashMap<String, VecDeque<Instant>>>>,
}

impl RateLimiter {
    pub async fn allow_query(&self, session_id: &str) -> Result<()> {
        let mut counts = self.session_query_counts.lock().await;
        let queue = counts.entry(session_id.to_string()).or_insert(VecDeque::new());
        
        // Remove queries older than 1 minute
        let one_min_ago = Instant::now() - Duration::from_secs(60);
        while queue.front().map(|&t| t < one_min_ago).unwrap_or(false) {
            queue.pop_front();
        }
        
        if queue.len() >= self.queries_per_minute as usize {
            return Err(RateLimitExceeded);
        }
        
        queue.push_back(Instant::now());
        Ok(())
    }
}

// Config
[security]
rate_limit_queries_per_minute = 10  # Default: 10 queries/min per session
```

### Audit Logging

Every query and result logged with full context:

```rust
#[instrument(
    skip(query, response),
    fields(
        query_id = %query.id,
        session_id = %session_id,
        query_kind = ?query.kind,
        client_ip = %client_ip,
        response_ok = response.error.is_none(),
        result_bytes = response.serialized_bytes(),
    )
)]
async fn log_query_execution(
	query: &ServerQuery,
	response: &ServerQueryResponse,
	session_id: &str,
	client_ip: &str,
) {
	info!("Server query executed");
}

// Logs enable:
// - Full audit trail for compliance
// - Detecting suspicious patterns (100 queries in 10 seconds)
// - Post-incident forensics
// - User behavior analytics
```

### Sandboxing

**File access sandboxing:**

```rust
impl AcpServerQueryHandler {
	fn validate_path(&self, requested_path: &str) -> Result<PathBuf> {
		let path = Path::new(requested_path);

		// Resolve to absolute
		let abs_path = if path.is_absolute() {
			path.to_path_buf()
		} else {
			self.workspace_root.join(path)
		};

		// Prevent directory traversal
		abs_path.canonicalize()?; // Resolve symlinks
		if !abs_path.starts_with(&self.workspace_root) {
			return Err(SecurityError::OutsideWorkspace);
		}

		Ok(abs_path)
	}
}

// Prevents reading /etc/passwd, /root/.ssh, etc.
// Only files within workspace_root are accessible
```

---

## 8. Future Enhancements

### Phase 2.5: ML-Based Query Detection

```rust
/// Uses lightweight ML model instead of regex
pub struct MlQueryDetectionStrategy {
	model: Arc<TfLiteModel>, // TensorFlow Lite
	classifier: Arc<QueryClassifier>,
}

impl QueryDetectionStrategy for MlQueryDetectionStrategy {
	fn detect_from_text(&self, text: &str, context: &ConversationContext) -> Option<ServerQuery> {
		// Tokenize & embed text
		let tokens = self.tokenizer.encode(text);
		let embedding = self.model.embed(&tokens)?;

		// Classify: [is_query, query_type, parameters]
		let predictions = self.model.infer(&embedding)?;

		if predictions.is_query_confidence > 0.85 {
			let query_type = self.classifier.classify(&predictions)?;
			let params = self.extractor.extract_params(text, &predictions)?;

			return Some(ServerQuery {
				kind: query_type,
				..
			});
		}

		None
	}
}

// Benefits:
// - Handles natural language variations better
// - Fewer false positives
// - Can learn from user feedback
// - Supports custom intent detection
```

### Phase 3: Query Batching

```rust
/// Batch multiple queries into single round-trip
pub async fn batch_queries(
	&self,
	session_id: &str,
	queries: Vec<ServerQuery>,
) -> Result<Vec<ServerQueryResponse>> {
	// Send all queries in one SSE event
	let batch = ServerQueryBatch {
		id: uuid7(),
		queries: queries.clone(),
		..
	};

	self.send_batch_to_client(session_id, &batch).await?;

	// Client processes all in parallel, responds with batch
	let responses = self
		.wait_for_batch_responses(&batch.id, queries.len())
		.await?;

	Ok(responses)
}

// Benefits:
// - Reduce round-trip latency (1 round-trip for N queries)
// - Client can parallelize (read 3 files at once)
// - Atomic: all queries must complete or all fail
```

### Phase 3: Result Caching

```rust
/// Cache query results per session
pub struct QueryResultCache {
	cache: Arc<Mutex<HashMap<String, CachedResult>>>,
	ttl: Duration,
}

pub struct CachedResult {
	query_hash: String, // Hash of (query_kind, parameters)
	result: ServerQueryResponse,
	cached_at: Instant,
}

impl QueryResultCache {
	pub async fn get_or_fetch(&self, query: &ServerQuery) -> Result<ServerQueryResponse> {
		let query_hash = self.hash_query(query);

		// Check cache
		if let Some(cached) = self.cache.lock().await.get(&query_hash) {
			if cached.cached_at.elapsed() < self.ttl {
				info!("Cache hit for query");
				return Ok(cached.result.clone());
			}
		}

		// Fetch fresh
		let response = self.query_manager.send_query(query).await?;

		// Store in cache
		self.cache.lock().await.insert(
			query_hash,
			CachedResult {
				query_hash,
				result: response.clone(),
				cached_at: Instant::now(),
			},
		);

		Ok(response)
	}
}

// Benefits:
// - "Get HOME" called multiple times? Cache it
// - "Read same file" twice? Use cached version
// - Reduces latency for common queries
// - Configurable TTL per query type
```

### Phase 3: Streaming Large Results

```rust
/// Stream large file results to client instead of loading all in memory
pub async fn stream_file_result(
	&self,
	query: &ServerQuery,
	file_path: &Path,
) -> Result<impl Stream<Item = Result<String>>> {
	// Read file in chunks
	let file = tokio::fs::File::open(file_path).await?;
	let reader = tokio::io::BufReader::new(file);
	let mut lines = reader.lines();

	// Yield chunks to SSE stream
	let stream = async_stream::stream! {
			while let Some(line) = lines.next_line().await? {
					yield Ok(line);
			}
	};

	Ok(stream)
}

// Benefits:
// - Read multi-MB files without loading all into memory
// - Stream to LLM token by token
// - Progressive display to user
// - Better latency for large operations
```

### Phase 4: Custom Query Types (User-Defined)

```rust
/// Users can register custom query handlers
pub trait CustomQueryHandler: Send + Sync {
    fn query_type(&self) -> &str;  // "get_db_schema", etc.
    
    async fn handle(
        &self,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value>;
}

impl LoomServer {
    pub fn register_custom_handler(&mut self, handler: Arc<dyn CustomQueryHandler>) {
        self.custom_handlers.insert(handler.query_type().to_string(), handler);
    }
}

// Usage:
pub struct GetDatabaseSchemaHandler;

impl CustomQueryHandler for GetDatabaseSchemaHandler {
    fn query_type(&self) -> &str { "get_db_schema" }
    
    async fn handle(&self, payload: serde_json::Value) -> Result<serde_json::Value> {
        let db_name = payload["database"].as_str()?;
        let schema = query_database_schema(db_name).await?;
        Ok(serde_json::json!(schema))
    }
}

// Server registers
server.register_custom_handler(Arc::new(GetDatabaseSchemaHandler));

// LLM detects: "Get the database schema for users_db"
// → ServerQuery { kind: Custom { name: "get_db_schema", payload: {...} } }
// → Custom handler invoked
// → Result injected into context
```

---

## Summary

Phase 2 transforms the Loom server-query system from a passive framework into an **active,
intelligent query system** that:

✅ **Automatically detects** when the LLM needs information\
✅ **Seamlessly injects** query results back into the conversation\
✅ **Maintains security** with sandboxing, rate limiting, and audit logs\
✅ **Extensible** via custom patterns and strategies\
✅ **Fully tested** with property-based and integration tests\
✅ **Production-ready** with structured logging and monitoring

**Estimated Timeline:** 4 days for core implementation + testing\
**Team Size:** 1-2 engineers\
**Risk:** Low (builds on stable Phase 1 foundation)\
**Impact:** High (enables autonomous file access, environment queries, user interaction)
