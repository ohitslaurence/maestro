<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# LLM Client Abstraction

## Overview

The `LlmClient` trait provides a unified interface for interacting with different LLM providers
(Anthropic Claude, OpenAI GPT, etc.). This abstraction enables:

- **Runtime polymorphism** via `Arc<dyn LlmClient>` for provider switching without recompilation
- **Consistent API** across providers for both blocking and streaming completions
- **Streaming support** via the `LlmStream` wrapper that abstracts provider-specific SSE formats

The core abstraction lives in [`crates/loom-core/src/llm.rs`](../crates/loom-core/src/llm.rs).

## Core Types

### LlmRequest

Request payload sent to an LLM for completion:

```rust
pub struct LlmRequest {
	pub model: String, // Model identifier (e.g., "claude-sonnet-4-20250514", "gpt-4o")
	pub messages: Vec<Message>, // Conversation history
	pub tools: Vec<ToolDefinition>, // Available tools for function calling
	pub max_tokens: Option<u32>, // Maximum response tokens
	pub temperature: Option<f32>, // Sampling temperature (0.0-2.0)
}
```

Builder pattern support:

```rust
LlmRequest::new("gpt-4o")
    .with_messages(messages)
    .with_tools(tools)
    .with_max_tokens(4096)
    .with_temperature(0.7)
```

### LlmResponse

Response from a completion request:

```rust
pub struct LlmResponse {
	pub message: Message,          // Assistant's text response
	pub tool_calls: Vec<ToolCall>, // Requested tool invocations
	pub usage: Option<Usage>,      // Token usage statistics
	pub finish_reason: Option<String>, /* Why generation stopped (e.g., "end_turn", "tool_use",
	                                * "stop") */
}
```

### LlmEvent

Streaming events emitted during completion:

```rust
pub enum LlmEvent {
	/// Incremental text content from the assistant
	TextDelta { content: String },

	/// Incremental tool call data (arguments streamed in fragments)
	ToolCallDelta {
		call_id: String,
		tool_name: String,
		arguments_fragment: String,
	},

	/// The completion has finished successfully
	Completed(LlmResponse),

	/// An error occurred during streaming
	Error(LlmError),
}
```

### LlmStream

Async stream wrapper for LLM events:

```rust
pub struct LlmStream {
	inner: Pin<Box<dyn Stream<Item = LlmEvent> + Send>>,
}

impl LlmStream {
	fn new(inner: Pin<Box<dyn Stream<Item = LlmEvent> + Send>>) -> Self;
	async fn next(&mut self) -> Option<LlmEvent>;
}

impl Stream for LlmStream {
	type Item = LlmEvent;
	// ...
}
```

Uses `pin_project_lite` for efficient pinning. Implements both:

- Direct async iteration via `next()`
- `futures::Stream` trait for combinator compatibility

### LlmError

Error variants for LLM operations (from
[`crates/loom-core/src/error.rs`](../crates/loom-core/src/error.rs)):

```rust
pub enum LlmError {
	#[error("HTTP error: {0}")]
	Http(String), // Network/transport failures

	#[error("API error: {0}")]
	Api(String), // Provider API errors (auth, validation)

	#[error("Request timed out")]
	Timeout, // Request timeout

	#[error("Invalid response: {0}")]
	InvalidResponse(String), // Parse/deserialization failures

	#[error("Rate limited: retry after {retry_after_secs:?} seconds")]
	RateLimited { retry_after_secs: Option<u64> }, // 429 responses
}
```

## LlmClient Trait

The async trait interface:

```rust
#[async_trait]
pub trait LlmClient: Send + Sync {
	/// Sends a completion request and waits for the full response.
	async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;

	/// Sends a completion request and returns a stream of events.
	async fn complete_streaming(&self, request: LlmRequest) -> Result<LlmStream, LlmError>;
}
```

## HTTP Proxy LLM Client

Located in [`crates/loom-llm-proxy/`](../crates/loom-llm-proxy/):

The `ProxyLlmClient` implements `LlmClient` by forwarding requests to the loom server's
provider-specific proxy endpoints. Clients no longer need API keys — they only need the server URL
and a provider selection.

**Configuration:**

```rust
pub struct ProxyLlmConfig {
	pub server_url: String,    // e.g., "http://localhost:3000"
	pub provider: LlmProvider, // Anthropic or OpenAI
}

 pub enum LlmProvider {
	Anthropic,
	OpenAI,
	Zai,
}
```

**Usage:**

```rust
// Convenience constructors for specific providers
let anthropic_client = ProxyLlmClient::anthropic("http://localhost:3000")?;
let openai_client = ProxyLlmClient::openai("http://localhost:3000")?;
let zai_client = ProxyLlmClient::zai("http://localhost:3000")?;

// Explicit provider selection
let client = ProxyLlmClient::new("http://localhost:3000", LlmProvider::Anthropic)?;

let response = client.complete(request).await?;
```

**Endpoints Called:**

For Anthropic provider:

- `complete()` → `POST /proxy/anthropic/complete`
- `complete_streaming()` → `POST /proxy/anthropic/stream`

For OpenAI provider:

- `complete()` → `POST /proxy/openai/complete`
- `complete_streaming()` → `POST /proxy/openai/stream`

For Z.ai provider:

- `complete()` → `POST /proxy/zai/complete`
- `complete_streaming()` → `POST /proxy/zai/stream`

### Wire Format

**POST /proxy/{provider}/complete** (e.g., `/proxy/anthropic/complete`, `/proxy/openai/complete`, `/proxy/zai/complete`)

Request body: `LlmRequest` JSON

```json
{
  "model": "claude-sonnet-4-20250514",
  "messages": [...],
  "tools": [...],
  "max_tokens": 4096,
  "temperature": 0.7
}
```

Response body: `LlmProxyResponse` JSON

```json
{
  "message": { "role": "assistant", "content": "..." },
  "tool_calls": [...],
  "usage": { "prompt_tokens": 100, "completion_tokens": 50 },
  "finish_reason": "end_turn"
}
```

**POST /proxy/{provider}/stream** (e.g., `/proxy/anthropic/stream`, `/proxy/openai/stream`, `/proxy/zai/stream`)

Request body: `LlmRequest` JSON (same as above)

Response: Server-Sent Events (SSE) stream with `LlmStreamEvent` payloads:

```
data: {"type": "text_delta", "content": "Hello"}
data: {"type": "tool_call_delta", "call_id": "...", "tool_name": "...", "arguments_fragment": "..."}
data: {"type": "completed", "response": {...}}
data: {"type": "error", "message": "..."}
```

## Server-Side LLM Service

Located in [`crates/loom-llm-service/`](../crates/loom-llm-service/):

The `LlmService` runs on the server and owns the provider clients. It supports multiple providers
simultaneously and exposes provider-specific methods.

**Configuration via environment variables:**

```bash
ANTHROPIC_API_KEY=sk-ant-...
OPENAI_API_KEY=sk-...
ZAI_API_KEY=...
# All providers can be configured at the same time
```

**Architecture:**

```rust
pub struct LlmService {
	anthropic_client: Option<AnthropicClient>,
	openai_client: Option<OpenAIClient>,
	zai_client: Option<ZaiClient>,
}

impl LlmService {
	fn from_env() -> Result<Self, LlmError>;

	// Provider availability checks
	fn has_anthropic(&self) -> bool;
	fn has_openai(&self) -> bool;
	fn has_zai(&self) -> bool;

	// Provider-specific completion methods
	async fn complete_anthropic(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;
	async fn complete_streaming_anthropic(&self, request: LlmRequest) -> Result<LlmStream, LlmError>;
	async fn complete_openai(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;
	async fn complete_streaming_openai(&self, request: LlmRequest) -> Result<LlmStream, LlmError>;
	async fn complete_zai(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;
	async fn complete_streaming_zai(&self, request: LlmRequest) -> Result<LlmStream, LlmError>;
}
```

The service supports having both Anthropic and OpenAI configured simultaneously. Clients explicitly
choose which provider to use via the endpoint path (`/proxy/anthropic/*` or `/proxy/openai/*`).

## Provider Implementations (Server-Only)

> **Note:** These provider clients are now **server-only** and are wrapped by `LlmService`. Client
> applications should use `ProxyLlmClient` instead of instantiating provider clients directly.

### AnthropicClient

Located in [`crates/loom-llm-anthropic/`](../crates/loom-llm-anthropic/):

**Configuration:**

```rust
pub struct AnthropicConfig {
	pub api_key: String,
	pub base_url: String, // Default: "https://api.anthropic.com"
	pub model: String,    // Default: "claude-sonnet-4-20250514"
}
```

**API Details:**

- Endpoint: `POST /v1/messages`
- Headers: `x-api-key`, `anthropic-version: 2023-06-01`
- System messages extracted to top-level `system` field
- Tool results sent as `tool_result` content blocks in user messages

**SSE Streaming Events:**

```
message_start → content_block_start → content_block_delta* → content_block_stop → message_delta → message_stop
```

Event types: `text_delta`, `input_json_delta` (for tool args), `ping`, `error`

### OpenAIClient

Located in [`crates/loom-llm-openai/`](../crates/loom-llm-openai/):

**Configuration:**

```rust
pub struct OpenAIConfig {
	pub api_key: String,
	pub base_url: String, // Default: "https://api.openai.com/v1"
	pub model: String,    // Default: "gpt-4o"
	pub organization: Option<String>,
}
```

**API Details:**

- Endpoint: `POST /chat/completions`
- Headers: `Authorization: Bearer {api_key}`, optional `OpenAI-Organization`
- Tool choice defaults to `"auto"` when tools are provided

**SSE Streaming Format:**

```
data: {"choices":[{"delta":{"content":"..."}}]}
data: {"choices":[{"delta":{"tool_calls":[...]}}]}
data: [DONE]
```

### ZaiClient

Located in [`crates/loom-llm-zai/`](../crates/loom-llm-zai/):

**Configuration:**

```rust
pub struct ZaiConfig {
	pub api_key: String,
	pub base_url: String, // Default: "https://api.z.ai/api/paas/v4"
	pub model: String,    // Default: "glm-4.7"
}
```

**API Details:**

- Endpoint: `POST /chat/completions`
- Headers: `Authorization: Bearer {api_key}`, optional `Accept-Language`
- OpenAI-compatible API format
- Supports models: `glm-4.7`, `glm-4.6`, `glm-4.5`, `glm-4.5-air`, `glm-4.5-x`, `glm-4.5-airx`, `glm-4.5-flash`, `glm-4-32b-0414-128k`

**SSE Streaming Format:**

```
data: {"choices":[{"delta":{"content":"..."}}]}
data: {"choices":[{"delta":{"tool_calls":[...]}}]}
data: [DONE]
```

Uses the same OpenAI-compatible streaming format.

## Message Format Conversion

### loom-core Message → Provider Formats

**Message structure:**

```rust
pub struct Message {
	pub role: Role, // System, User, Assistant, Tool
	pub content: String,
	pub tool_call_id: Option<String>, // For Tool role responses
	pub name: Option<String>,         // Tool name for Tool role
}
```

**Anthropic Conversion** ([`types.rs`](../crates/loom-llm-anthropic/src/types.rs)):

- `Role::System` → extracted to top-level `system` field (not in messages array)
- `Role::User` → `{"role": "user", "content": "..."}`
- `Role::Assistant` → `{"role": "assistant", "content": "..."}`
- `Role::Tool` →
  `{"role": "user", "content": [{"type": "tool_result", "tool_use_id": "...", "content": "..."}]}`

**OpenAI Conversion** ([`types.rs`](../crates/loom-llm-openai/src/types.rs)):

- Direct role mapping: `system`, `user`, `assistant`, `tool`
- `Role::Tool` includes `tool_call_id` and `name` fields

**Z.ai Conversion** ([`types.rs`](../crates/loom-llm-zai/src/types.rs)):

- Direct role mapping (OpenAI-compatible): `system`, `user`, `assistant`, `tool`
- `Role::Tool` includes `tool_call_id` and `name` fields
- Same format as OpenAI (Z.ai API is OpenAI-compatible)

## Tool Definition Conversion

**loom-core ToolDefinition:**

```rust
pub struct ToolDefinition {
	pub name: String,
	pub description: String,
	pub input_schema: serde_json::Value, // JSON Schema
}
```

**Anthropic Format:**

```json
{
  "name": "get_weather",
  "description": "Get current weather",
  "input_schema": { "type": "object", "properties": {...} }
}
```

**OpenAI Format:**

```json
{
  "type": "function",
  "function": {
    "name": "get_weather",
    "description": "Get current weather",
    "parameters": { "type": "object", "properties": {...} }
  }
}
```

**Z.ai Format:**

```json
{
  "type": "function",
  "function": {
    "name": "get_weather",
    "description": "Get current weather",
    "parameters": { "type": "object", "properties": {...} }
  }
}
```

Z.ai uses the same OpenAI-compatible format.

## Design Decisions

### Why async_trait

The `#[async_trait]` macro is required because Rust doesn't yet support async functions in traits
natively. It desugars async trait methods to return `Pin<Box<dyn Future>>`, enabling:

- Trait object safety (`dyn LlmClient`)
- Dynamic dispatch at runtime

```rust
#[async_trait]
pub trait LlmClient: Send + Sync {
	async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;
}
```

### Why Arc<dyn LlmClient> for Runtime Polymorphism

Using `Arc<dyn LlmClient>` allows:

1. **Configuration-driven provider selection** without compile-time generics
2. **Shared ownership** across async tasks (Arc provides thread-safe reference counting)
3. **Late binding** — switch providers based on runtime configuration

**Client-side (uses proxy with explicit provider):**

```rust
// Explicit provider selection
let client: Arc<dyn LlmClient> = Arc::new(ProxyLlmClient::new(
    config.server_url.clone(),
    LlmProvider::Anthropic,
)?);

let client: Arc<dyn LlmClient> = Arc::new(ProxyLlmClient::new(
    config.server_url.clone(),
    LlmProvider::Zai,
)?);
```

**Server-side (LlmService with provider-specific methods):**

```rust
let service = LlmService::from_env()?;

// Check provider availability
if service.has_anthropic() {
    let response = service.complete_anthropic(request).await?;
}
if service.has_openai() {
    let response = service.complete_openai(request).await?;
}
if service.has_zai() {
    let response = service.complete_zai(request).await?;
}
```

### How Streaming is Abstracted via LlmStream

Each provider has its own SSE parsing logic that:

1. Receives raw `bytes::Bytes` chunks from the HTTP response
2. Buffers and parses SSE event frames (`data: {...}\n\n`)
3. Deserializes provider-specific event types
4. Converts to unified `LlmEvent` variants
5. Tracks accumulated state (content, tool calls, usage)
6. Emits `LlmEvent::Completed(LlmResponse)` when stream ends

The `LlmStream` wrapper erases the provider-specific stream type:

```rust
let boxed: Pin<Box<dyn Stream<Item = LlmEvent> + Send>> = Box::pin(provider_stream);
Ok(LlmStream::new(boxed))
```

## Adding New Providers

### ZaiClient Implementation

ZaiClient is implemented in [`crates/loom-llm-zai/`](../crates/loom-llm-zai/) using the OpenAI-compatible Z.ai API (智谱AI/ZhipuAI).

**Key Features:**

- OpenAI-compatible API format
- Base URL: `https://api.z.ai/api/paas/v4`
- Default model: `glm-4.7`
- Streaming via SSE with `data: [DONE]` marker
- Tool calling support
- Models: `glm-4.7`, `glm-4.6`, `glm-4.5`, `glm-4.5-air`, `glm-4.5-x`, `glm-4.5-airx`, `glm-4.5-flash`, `glm-4-32b-0414-128k`

### Step-by-Step Guide: Adding Google Gemini

1. **Create a crate structure:**
    ```
    crates/loom-llm-gemini/
    ├── Cargo.toml
    └── src/
        ├── lib.rs
        ├── client.rs
        ├── types.rs
        └── stream.rs
    ```

2. **Add dependencies in `Cargo.toml`:**
    ```toml
    [package]
    name = "loom-llm-gemini"
    version = "0.1.0"
    edition = "2021"

    [dependencies]
    loom-core = { path = "../loom-core" }
    loom-http = { path = "../loom-http" }
    async-trait = "0.1"
    bytes = "1"
    futures = "0.3"
    pin-project-lite = "0.2"
    reqwest = { version = "0.11", features = ["json", "stream"] }
    serde = { version = "1", features = ["derive"] }
    serde_json = "1"
    tracing = "0.1"
    ```

3. **Define configuration in `types.rs`:**
    ```rust
    pub struct GeminiConfig {
    	pub api_key: String,
    	pub base_url: String, // "https://generativelanguage.googleapis.com"
    	pub model: String,    // "gemini-pro"
    }
    ```

4. **Define API types in `types.rs`:**
    - `GeminiRequest` — map from `LlmRequest`
    - `GeminiResponse` — map to `LlmResponse`
    - `GeminiMessage`, `GeminiContent`, `GeminiFunctionCall`, etc.
    - Implement `From<&LlmRequest>` and `TryFrom<GeminiResponse>`

5. **Implement client in `client.rs`:**
    ```rust
    pub struct GeminiClient {
    	config: GeminiConfig,
    	http_client: Client,
    	retry_config: RetryConfig,
    }

    #[async_trait]
    impl LlmClient for GeminiClient {
    	async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
    		// 1. Convert LlmRequest → GeminiRequest
    		// 2. POST to /v1beta/models/{model}:generateContent
    		// 3. Parse GeminiResponse → LlmResponse
    	}

    	async fn complete_streaming(&self, request: LlmRequest) -> Result<LlmStream, LlmError> {
    		// 1. Convert LlmRequest → GeminiRequest with stream=true
    		// 2. POST to /v1beta/models/{model}:streamGenerateContent
    		// 3. Wrap response in GeminiStream
    		// 4. Return LlmStream::new(Box::pin(stream))
    	}
    }
    ```

6. **Implement SSE parsing in `stream.rs`:**
    ```rust
    pin_project! {
    		pub struct GeminiStream<S> {
    				#[pin]
    				inner: S,
    				buffer: String,
    				state: StreamState,
    				finished: bool,
    		}
    }

    impl<S, E> Stream for GeminiStream<S>
    where
    	S: Stream<Item = Result<Bytes, E>>,
    	E: std::error::Error,
    {
    	type Item = LlmEvent;

    	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    		// Parse Gemini's streaming format
    		// Emit TextDelta, ToolCallDelta, Completed, or Error
    	}
    }
    ```

7. **Export from `lib.rs`:**
    ```rust
    mod client;
    mod stream;
    mod types;

    pub use client::GeminiClient;
    pub use types::GeminiConfig;
    ```

8. **Add to workspace `Cargo.toml`:**
    ```toml
    [workspace]
    members = [
    	"crates/loom-llm-gemini",
    	# ...
    ]
    ```

9. **Write tests:**
    - Unit tests for type conversions
    - Unit tests for SSE parsing (mock byte streams)
    - Integration tests with mock server (optional)

