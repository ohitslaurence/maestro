<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->

# SSE Streaming Design

## Overview

Streaming is essential for LLM-based applications because large language models generate responses
token-by-token. Without streaming, users would wait for the entire response to complete before
seeing any output—potentially seconds or even minutes of delay for complex responses.

**User Experience Benefits:**

- **Immediate feedback**: Users see the first words within milliseconds, reducing perceived latency
- **Progressive disclosure**: Text appears naturally, similar to watching someone type
- **Early cancellation**: Users can abort requests as soon as they see the response isn't what they
  need
- **Tool call visibility**: Watch tool arguments stream in real-time, understanding what the LLM is
  attempting

## Server-Sent Events (SSE)

SSE is a W3C standard for unidirectional server-to-client streaming over HTTP. Unlike WebSockets,
SSE uses standard HTTP and is simpler to implement and debug.

**Protocol Structure:**

```
event: message_start
data: {"type":"message_start","message":{...}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}
```

Key characteristics:

- Each event is separated by double newlines (`\n\n`)
- Lines prefixed with `data:` contain the payload
- Lines prefixed with `event:` optionally specify event type
- Lines prefixed with `:` are comments (used for keep-alive pings)

## LlmEvent Discriminated Union

The [`LlmEvent`](file:///home/ghuntley/loom/crates/loom-core/src/llm.rs#L62-L75) enum provides a
unified streaming interface across all LLM providers:

```rust
pub enum LlmEvent {
	/// Incremental text content from the assistant.
	TextDelta { content: String },

	/// Incremental tool call data.
	ToolCallDelta {
		call_id: String,
		tool_name: String,
		arguments_fragment: String,
	},

	/// The completion has finished successfully.
	Completed(LlmResponse),

	/// An error occurred during streaming.
	Error(LlmError),
}
```

### TextDelta

Contains a fragment of the assistant's text response. Consumers concatenate these fragments to build
the complete message.

### ToolCallDelta

Streams partial tool call arguments. The `call_id` and `tool_name` identify which tool is being
invoked, while `arguments_fragment` contains a JSON fragment that must be accumulated until the tool
call completes.

### Completed

Signals successful stream completion. Contains the final
[`LlmResponse`](file:///home/ghuntley/loom/crates/loom-core/src/llm.rs#L78-L84) with:

- Complete message content
- All accumulated tool calls with parsed arguments
- Token usage statistics
- Finish reason (e.g., "end_turn", "tool_use", "max_tokens")

### Error

Propagates errors that occur during streaming, wrapped in
[`LlmError`](file:///home/ghuntley/loom/crates/loom-core/src/error.rs).

## LlmStream Implementation

The [`LlmStream`](file:///home/ghuntley/loom/crates/loom-core/src/llm.rs#L93-L124) type wraps
provider-specific streams into a unified interface:

```rust
pin_project! {
		pub struct LlmStream {
				#[pin]
				inner: Pin<Box<dyn Stream<Item = LlmEvent> + Send>>,
		}
}
```

### pin_project_lite Usage

The `pin_project!` macro from [`pin_project_lite`](https://docs.rs/pin-project-lite) generates safe
pin projections for the struct. This is necessary because:

1. The inner stream is pinned (`Pin<Box<dyn Stream>>`)
2. `Stream::poll_next` requires `Pin<&mut Self>`
3. Manual pin projection is error-prone and unsafe

### Stream Trait Implementation

```rust
impl Stream for LlmStream {
	type Item = LlmEvent;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		self.project().inner.poll_next(cx)
	}
}
```

The implementation delegates directly to the inner stream, enabling use with any async stream
combinator from `futures`.

### Async Iteration

The `next()` method provides ergonomic async iteration:

```rust
pub async fn next(&mut self) -> Option<LlmEvent> {
	use futures::StreamExt;
	self.inner.next().await
}
```

Usage:

```rust
let mut stream = client.complete_streaming(request).await?;
while let Some(event) = stream.next().await {
    match event {
        LlmEvent::TextDelta { content } => print!("{}", content),
        LlmEvent::Completed(response) => break,
        LlmEvent::Error(e) => return Err(e),
        _ => {}
    }
}
```

## Anthropic SSE Format

The Anthropic streaming API uses a rich event structure. See
[`crates/loom-llm-anthropic/src/stream.rs`](file:///home/ghuntley/loom/crates/loom-llm-anthropic/src/stream.rs).

### Event Types

| Event                 | Purpose                                                      |
| --------------------- | ------------------------------------------------------------ |
| `message_start`       | Stream begins, contains message ID, model, input token count |
| `content_block_start` | New content block (text or tool_use) begins                  |
| `content_block_delta` | Incremental content (`text_delta` or `input_json_delta`)     |
| `content_block_stop`  | Content block completed                                      |
| `message_delta`       | Message-level updates (stop_reason, output tokens)           |
| `message_stop`        | Stream complete                                              |
| `ping`                | Keep-alive signal                                            |
| `error`               | API error during streaming                                   |

### Tool Use Block Accumulation

Tool calls are handled via content blocks:

1. `content_block_start` with `type: "tool_use"` announces a tool call with its `id` and `name`
2. `content_block_delta` events with `input_json_delta` stream the JSON arguments
3. `content_block_stop` signals the tool call is complete

```rust
struct ToolCallBuilder {
	id: String,
	name: String,
	arguments_json: String, // Accumulated JSON fragments
}
```

### State Tracking

The [`StreamState`](file:///home/ghuntley/loom/crates/loom-llm-anthropic/src/stream.rs#L89-L97)
struct maintains parsing context:

```rust
struct StreamState {
	tool_calls: HashMap<usize, ToolCallBuilder>, // Index -> builder
	accumulated_content: String,
	accumulated_tool_calls: Vec<ToolCall>,
	stop_reason: Option<String>,
	input_tokens: u32,
	output_tokens: u32,
}
```

The `index` field in events maps content blocks to their builders, allowing multiple tool calls to
stream concurrently.

## OpenAI SSE Format

The OpenAI streaming API uses a simpler line-based format. See
[`crates/loom-llm-openai/src/stream.rs`](file:///home/ghuntley/loom/crates/loom-llm-openai/src/stream.rs).

### Data-Prefixed Lines

Every payload line is prefixed with `data:`:

```
data: {"id":"chatcmpl-123","choices":[{"delta":{"content":"Hello"}}]}

data: {"id":"chatcmpl-123","choices":[{"delta":{"content":" world"}}]}

data: [DONE]
```

### [DONE] Marker

The literal string `data: [DONE]` signals stream completion. This must be handled specially—it's not
valid JSON:

```rust
if data == "[DONE]" {
    *finished = true;
    let response = build_final_response(...);
    return Some(LlmEvent::Completed(response));
}
```

### Delta Accumulation

OpenAI streams tool calls with indexed deltas:

```rust
struct AccumulatedToolCall {
    id: String,
    name: String,
    arguments: String,  // Accumulated JSON string
}

accumulated_tool_calls: HashMap<u32, AccumulatedToolCall>
```

Tool call deltas arrive with:

- `index`: Which tool call this delta belongs to
- `id`: Tool call ID (only in first delta)
- `function.name`: Function name (only in first delta)
- `function.arguments`: JSON fragment to append

## Server-to-Client SSE Proxy

The loom server acts as a proxy between LLM providers and clients, exposing provider-specific
endpoints while normalizing SSE formats into a unified wire format.

### Architecture Flow

```
Provider API → loom-llm-{anthropic,openai} → loom-llm-service → loom-server → SSE → loom-llm-proxy → client
                                              ↓                   ↓
                                         complete_anthropic()  /proxy/anthropic/stream
                                         complete_openai()     /proxy/openai/stream
```

Provider-specific SSE parsing happens server-side in `loom-llm-anthropic` and `loom-llm-openai`.
Clients choose their provider via endpoint path (`/proxy/anthropic/stream` or
`/proxy/openai/stream`) and receive a unified `LlmStreamEvent` format.

### LlmStreamEvent Wire Format

The server emits SSE events with `event: llm` and JSON payloads discriminated by `type`:

```
event: llm
data: {"type":"text_delta","content":"Hello"}

event: llm
data: {"type":"tool_call_delta","call_id":"...","tool_name":"...","arguments_fragment":"..."}

event: llm
data: {"type":"completed","response":{...}}

event: llm
data: {"type":"error","message":"..."}
```

| Type              | Description                                          |
| ----------------- | ---------------------------------------------------- |
| `text_delta`      | Incremental text content from the assistant          |
| `tool_call_delta` | Partial tool call with ID, name, and JSON fragment   |
| `completed`       | Stream finished successfully with full `LlmResponse` |
| `error`           | Error occurred during streaming                      |

### SSE Format Comparison

| Layer                    | SSE Format               | Parser            | Endpoint                  |
| ------------------------ | ------------------------ | ----------------- | ------------------------- |
| Anthropic API            | Anthropic-specific       | `AnthropicStream` | -                         |
| OpenAI API               | OpenAI-specific          | `OpenAIStream`    | -                         |
| Server Proxy (Anthropic) | Unified `LlmStreamEvent` | `ProxyLlmStream`  | `/proxy/anthropic/stream` |
| Server Proxy (OpenAI)    | Unified `LlmStreamEvent` | `ProxyLlmStream`  | `/proxy/openai/stream`    |

## ProxyLlmStream

The [`ProxyLlmStream`](file:///home/ghuntley/loom/crates/loom-llm-proxy/src/stream.rs) in
`loom-llm-proxy` parses the server's unified SSE format for client-side consumption.

### Responsibilities

1. **Parse SSE events**: Extract `event: llm` and `data:` lines from the byte stream
2. **Deserialize LlmStreamEvent**: Parse JSON payloads into the wire format types
3. **Convert to LlmEvent**: Map `LlmStreamEvent` variants to the internal `LlmEvent` enum
4. **Extract LlmResponse**: Return the completed `LlmResponse` from the `Completed` event

### Conversion

```rust
fn convert_stream_event(event: LlmStreamEvent) -> LlmEvent {
	match event {
		LlmStreamEvent::TextDelta { content } => LlmEvent::TextDelta { content },
		LlmStreamEvent::ToolCallDelta {
			call_id,
			tool_name,
			arguments_fragment,
		} => LlmEvent::ToolCallDelta {
			call_id,
			tool_name,
			arguments_fragment,
		},
		LlmStreamEvent::Completed { response } => LlmEvent::Completed(response),
		LlmStreamEvent::Error { message } => LlmEvent::Error(LlmError::Stream(message)),
	}
}
```

### Benefits

- **Single client implementation**: Clients only need to understand one SSE format
- **Provider isolation**: Provider-specific quirks are handled server-side
- **Simplified testing**: Mock the unified format without provider dependencies

## Design Decisions

### Why Custom Stream Parsers

We implement custom SSE parsers rather than using libraries like `eventsource-client` because:

1. **Provider-specific formats**: Anthropic and OpenAI have different event structures that require
   custom deserialization
2. **State management**: Tool call accumulation requires maintaining state across events
3. **Error handling**: Provider-specific error responses need custom parsing
4. **Control**: Direct access to the byte stream enables fine-grained buffering and backpressure

### Partial Tool Argument Accumulation

Tool arguments stream as JSON fragments that may be syntactically incomplete:

```
{"loc        <- Fragment 1: invalid JSON
ation":"    <- Fragment 2: still invalid
NYC"}       <- Fragment 3: now valid when concatenated
```

We accumulate fragments in a String buffer, only parsing to `serde_json::Value` when the tool call
completes (on `content_block_stop` for Anthropic, or `[DONE]` for OpenAI).

### Error Propagation in Streams

Errors during streaming are emitted as `LlmEvent::Error` rather than causing immediate stream
termination. This allows:

1. Upstream code to receive partial content before the error
2. Graceful error display to users
3. Potential recovery for transient errors

The `finished` flag prevents further polling after an error:

```rust
if matches!(llm_event, LlmEvent::Completed(_) | LlmEvent::Error(_)) {
    *this.finished = true;
}
```

## Extending for New Providers

To add streaming support for a new LLM provider:

### 1. Create Stream Types

```rust
// crates/loom-llm-newprovider/src/stream.rs

pin_project! {
		pub struct NewProviderStream<S> {
				#[pin]
				inner: S,
				buffer: String,
				// Provider-specific accumulation state
				state: StreamState,
				finished: bool,
		}
}
```

### 2. Implement Stream Trait

```rust
impl<S, E> Stream for NewProviderStream<S>
where
	S: Stream<Item = Result<Bytes, E>>,
	E: std::error::Error,
{
	type Item = LlmEvent;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		// 1. Check finished flag
		// 2. Try parsing buffered events
		// 3. Poll inner stream for more bytes
		// 4. Handle stream completion
	}
}
```

### 3. Parse Provider-Specific Format

Define serde types matching the provider's SSE format:

```rust
#[derive(Deserialize)]
struct ProviderEvent {
	// Match the provider's JSON structure
}
```

### 4. Map to LlmEvent

Convert provider events to the unified `LlmEvent` enum:

```rust
fn process_event(event: ProviderEvent, state: &mut State) -> Option<LlmEvent> {
	match event {
		ProviderEvent::TextChunk { text } => Some(LlmEvent::TextDelta { content: text }),
		ProviderEvent::Done => Some(LlmEvent::Completed(build_response(state))),
		// ...
	}
}
```

### 5. Expose Factory Function

```rust
pub fn parse_sse_stream<S, E>(stream: S) -> impl Stream<Item = LlmEvent>
where
	S: Stream<Item = Result<Bytes, E>>,
	E: std::error::Error,
{
	NewProviderStream::new(stream)
}
```

### 6. Integrate with Client

In the provider's `LlmClient::complete_streaming` implementation:

```rust
async fn complete_streaming(&self, request: LlmRequest) -> Result<LlmStream, LlmError> {
	let response = self.http_client.post(url).send().await?;
	let byte_stream = response.bytes_stream();
	let event_stream = parse_sse_stream(byte_stream);
	Ok(LlmStream::new(Box::pin(event_stream)))
}
```

### Testing Considerations

Write property-based tests verifying:

- Text deltas accumulate correctly across arbitrary chunk boundaries
- Tool call fragments parse to valid JSON when complete
- The `[DONE]` or equivalent marker produces `Completed` event
- Error events propagate and terminate the stream
- Stream handles malformed/partial UTF-8 gracefully
