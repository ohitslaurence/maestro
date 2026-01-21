//! Claude Code SDK to StreamEvent adapter (streaming-event-schema.md §2, §5)
//!
//! Maps Claude Code SDK messages (from Bun wrapper) to the unified StreamEvent schema.
//! Maintains stream ordering state (streamId + seq) per session.
//!
//! SDK Message Types (per specs/research/CLAUDE_CODE_RESEARCH.md §4):
//! - SDKAssistantMessage: Claude's response with content blocks
//! - SDKResultMessage: Final result with usage stats
//! - SDKPartialAssistantMessage: Streaming partial messages
//!
//! Content Block Types:
//! - text: Maps to StreamEventType::TextDelta
//! - thinking: Maps to StreamEventType::ThinkingDelta
//! - tool_use: Maps to StreamEventType::ToolCallDelta
//! - tool_result: Maps to StreamEventType::ToolCallCompleted

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Deserialize;
use serde_json::Value;

use crate::sessions::{
    AgentProcessingState, CompletedPayload, CompletionReason, ErrorPayload, StatusPayload,
    StreamErrorCode, StreamEvent, StreamEventType, TextDeltaPayload, ThinkingDeltaPayload,
    TokenUsage, ToolCallCompletedPayload, ToolCallDeltaPayload, ToolCallStatus,
};

// ============================================================================
// Claude Code SDK Message Structures (per research §4)
// ============================================================================

/// SDK message wrapper from Bun process.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SdkMessage {
    /// Claude's response with content blocks
    Assistant(SdkAssistantMessage),
    /// Final result with usage stats
    Result(SdkResultMessage),
    /// Streaming partial message (when includePartialMessages is enabled)
    PartialAssistant(SdkPartialAssistantMessage),
    /// Stream event (raw Anthropic API event)
    StreamEvent(SdkStreamEvent),
    /// User message (usually not needed for adapter)
    User(SdkUserMessage),
    /// System message
    System(SdkSystemMessage),
}

/// SDK assistant message with content blocks.
#[derive(Debug, Deserialize)]
pub struct SdkAssistantMessage {
    pub uuid: String,
    pub session_id: String,
    pub message: SdkMessageContent,
    pub parent_tool_use_id: Option<String>,
}

/// SDK message content wrapper.
#[derive(Debug, Deserialize)]
pub struct SdkMessageContent {
    pub role: String,
    pub content: Vec<ContentBlock>,
}

/// Content block types from Claude responses.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Text response
    Text { text: String },
    /// Extended thinking/reasoning
    Thinking { thinking: String },
    /// Tool invocation request
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    /// Tool execution result
    ToolResult {
        tool_use_id: String,
        content: Value,
        #[serde(default)]
        is_error: bool,
    },
}

/// Final result message with usage statistics.
#[derive(Debug, Deserialize)]
pub struct SdkResultMessage {
    pub subtype: String,
    pub session_id: String,
    pub duration_ms: u64,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub num_turns: u32,
    pub result: Option<String>,
    #[serde(default)]
    pub total_cost_usd: f64,
    pub usage: Option<SdkUsage>,
}

/// Token usage from SDK result.
#[derive(Debug, Deserialize)]
pub struct SdkUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
}

/// Partial assistant message for streaming.
#[derive(Debug, Deserialize)]
pub struct SdkPartialAssistantMessage {
    pub uuid: String,
    pub session_id: String,
    pub message: SdkMessageContent,
}

/// Raw stream event from Anthropic API.
#[derive(Debug, Deserialize)]
pub struct SdkStreamEvent {
    pub event: Value,
}

/// User message (for completeness).
#[derive(Debug, Deserialize)]
pub struct SdkUserMessage {
    pub uuid: String,
    pub session_id: String,
}

/// System message (for completeness).
#[derive(Debug, Deserialize)]
pub struct SdkSystemMessage {
    pub session_id: Option<String>,
}

// ============================================================================
// Stream State Tracking (§3, §5)
// ============================================================================

/// Per-stream ordering state.
/// streamId is stable per assistant response (per §3).
struct StreamState {
    stream_id: String,
    seq: AtomicU64,
}

impl StreamState {
    fn new(message_uuid: &str) -> Self {
        // Use message UUID as streamId since it's stable per assistant response
        Self {
            stream_id: format!("stream_{}", message_uuid),
            seq: AtomicU64::new(0),
        }
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }
}

/// Adapter state for a session.
/// Tracks active streams by message UUID for ordering.
#[derive(Default)]
pub struct SessionStreamState {
    /// Active streams by message UUID
    streams: HashMap<String, StreamState>,
    /// Track completed tool calls to avoid duplicate events
    completed_tools: std::collections::HashSet<String>,
}

impl SessionStreamState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create stream state for a message.
    fn get_or_create_stream(&mut self, message_uuid: &str) -> &StreamState {
        self.streams
            .entry(message_uuid.to_string())
            .or_insert_with(|| StreamState::new(message_uuid))
    }

    /// Mark a tool call as completed.
    fn mark_tool_completed(&mut self, tool_use_id: &str) -> bool {
        self.completed_tools.insert(tool_use_id.to_string())
    }

    /// Check if a tool call was already completed.
    #[allow(dead_code)]
    fn is_tool_completed(&self, tool_use_id: &str) -> bool {
        self.completed_tools.contains(tool_use_id)
    }
}

/// Global adapter state across sessions.
#[derive(Default)]
pub struct ClaudeCodeAdapter {
    sessions: std::sync::Mutex<HashMap<String, SessionStreamState>>,
}

impl ClaudeCodeAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create session state.
    fn with_session<F, R>(&self, session_id: &str, f: F) -> R
    where
        F: FnOnce(&mut SessionStreamState) -> R,
    {
        let mut sessions = self.sessions.lock().unwrap();
        let state = sessions
            .entry(session_id.to_string())
            .or_insert_with(SessionStreamState::new);
        f(state)
    }

    /// Adapt a Claude Code SDK message to StreamEvent(s).
    ///
    /// Returns None if the message should not be converted.
    pub fn adapt(&self, raw: &Value) -> Option<Vec<StreamEvent>> {
        let message: SdkMessage = serde_json::from_value(raw.clone()).ok()?;

        match message {
            SdkMessage::Assistant(msg) => self.adapt_assistant_message(&msg),
            SdkMessage::Result(msg) => self.adapt_result_message(&msg),
            SdkMessage::PartialAssistant(msg) => self.adapt_partial_message(&msg),
            SdkMessage::StreamEvent(_) => None, // Raw events are too low-level
            SdkMessage::User(_) | SdkMessage::System(_) => None, // Not needed
        }
    }

    /// Adapt an assistant message with content blocks.
    fn adapt_assistant_message(&self, msg: &SdkAssistantMessage) -> Option<Vec<StreamEvent>> {
        let session_id = &msg.session_id;
        let message_uuid = &msg.uuid;
        let mut events = Vec::new();

        for block in &msg.message.content {
            let (stream_id, seq) = self.with_session(session_id, |state| {
                let stream = state.get_or_create_stream(message_uuid);
                (stream.stream_id.clone(), stream.next_seq())
            });

            let event = match block {
                ContentBlock::Text { text } => {
                    let payload = TextDeltaPayload {
                        text: text.clone(),
                        role: "assistant".to_string(),
                    };
                    StreamEvent::new(
                        session_id.clone(),
                        "claude_code".to_string(),
                        stream_id,
                        seq,
                        StreamEventType::TextDelta,
                        serde_json::to_value(&payload).ok()?,
                    )
                    .with_message_id(message_uuid.clone())
                }
                ContentBlock::Thinking { thinking } => {
                    let payload = ThinkingDeltaPayload {
                        text: thinking.clone(),
                    };
                    StreamEvent::new(
                        session_id.clone(),
                        "claude_code".to_string(),
                        stream_id,
                        seq,
                        StreamEventType::ThinkingDelta,
                        serde_json::to_value(&payload).ok()?,
                    )
                    .with_message_id(message_uuid.clone())
                }
                ContentBlock::ToolUse { id, name, input } => {
                    // Tool use is a "delta" since we may receive partial input
                    let payload = ToolCallDeltaPayload {
                        call_id: id.clone(),
                        tool_name: name.clone(),
                        arguments_delta: serde_json::to_string(input).unwrap_or_default(),
                    };
                    StreamEvent::new(
                        session_id.clone(),
                        "claude_code".to_string(),
                        stream_id,
                        seq,
                        StreamEventType::ToolCallDelta,
                        serde_json::to_value(&payload).ok()?,
                    )
                    .with_message_id(message_uuid.clone())
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    // Check for duplicate completion
                    let already_completed = self.with_session(session_id, |state| {
                        !state.mark_tool_completed(tool_use_id)
                    });

                    if already_completed {
                        continue;
                    }

                    let output = match content {
                        Value::String(s) => s.clone(),
                        other => serde_json::to_string(other).unwrap_or_default(),
                    };

                    let status = if *is_error {
                        ToolCallStatus::Failed
                    } else {
                        ToolCallStatus::Completed
                    };

                    let payload = ToolCallCompletedPayload {
                        call_id: tool_use_id.clone(),
                        tool_name: String::new(), // Not available in tool_result
                        arguments: Value::Null,   // Not available in tool_result
                        output,
                        status,
                        error_message: if *is_error {
                            Some("Tool execution failed".to_string())
                        } else {
                            None
                        },
                    };
                    StreamEvent::new(
                        session_id.clone(),
                        "claude_code".to_string(),
                        stream_id,
                        seq,
                        StreamEventType::ToolCallCompleted,
                        serde_json::to_value(&payload).ok()?,
                    )
                    .with_message_id(message_uuid.clone())
                }
            };

            events.push(event);
        }

        if events.is_empty() {
            None
        } else {
            Some(events)
        }
    }

    /// Adapt a result message to completed event.
    fn adapt_result_message(&self, msg: &SdkResultMessage) -> Option<Vec<StreamEvent>> {
        let session_id = &msg.session_id;

        // Determine completion reason from subtype
        let reason = match msg.subtype.as_str() {
            "success" => CompletionReason::Stop,
            "error_max_turns" => CompletionReason::Length,
            "error_during_execution" => CompletionReason::ToolError,
            "error_max_budget_usd" => CompletionReason::Length,
            _ => CompletionReason::Stop,
        };

        let usage = msg.usage.as_ref().map(|u| TokenUsage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            reasoning_tokens: None,
        }).unwrap_or(TokenUsage {
            input_tokens: 0,
            output_tokens: 0,
            reasoning_tokens: None,
        });

        let payload = CompletedPayload { reason, usage };

        // For result messages, use session-based stream_id
        let stream_id = format!("result_{}", session_id);

        let event = StreamEvent::new(
            session_id.clone(),
            "claude_code".to_string(),
            stream_id,
            0, // Result is terminal, seq doesn't matter
            StreamEventType::Completed,
            serde_json::to_value(&payload).ok()?,
        );

        // If there was an error, also emit an error event
        if msg.is_error {
            let error_payload = ErrorPayload {
                code: StreamErrorCode::ProviderError,
                message: msg.result.clone().unwrap_or_else(|| "Unknown error".to_string()),
                recoverable: true,
                details: None,
            };

            let error_stream_id = format!("error_{}", session_id);
            let error_event = StreamEvent::new(
                session_id.clone(),
                "claude_code".to_string(),
                error_stream_id,
                0,
                StreamEventType::Error,
                serde_json::to_value(&error_payload).ok()?,
            );

            return Some(vec![error_event, event]);
        }

        Some(vec![event])
    }

    /// Adapt partial assistant messages (streaming).
    fn adapt_partial_message(&self, msg: &SdkPartialAssistantMessage) -> Option<Vec<StreamEvent>> {
        // Partial messages have the same structure as full assistant messages
        // but may have incomplete content blocks
        let session_id = &msg.session_id;
        let message_uuid = &msg.uuid;
        let mut events = Vec::new();

        for block in &msg.message.content {
            let (stream_id, seq) = self.with_session(session_id, |state| {
                let stream = state.get_or_create_stream(message_uuid);
                (stream.stream_id.clone(), stream.next_seq())
            });

            let event = match block {
                ContentBlock::Text { text } => {
                    let payload = TextDeltaPayload {
                        text: text.clone(),
                        role: "assistant".to_string(),
                    };
                    StreamEvent::new(
                        session_id.clone(),
                        "claude_code".to_string(),
                        stream_id,
                        seq,
                        StreamEventType::TextDelta,
                        serde_json::to_value(&payload).ok()?,
                    )
                    .with_message_id(message_uuid.clone())
                }
                ContentBlock::Thinking { thinking } => {
                    let payload = ThinkingDeltaPayload {
                        text: thinking.clone(),
                    };
                    StreamEvent::new(
                        session_id.clone(),
                        "claude_code".to_string(),
                        stream_id,
                        seq,
                        StreamEventType::ThinkingDelta,
                        serde_json::to_value(&payload).ok()?,
                    )
                    .with_message_id(message_uuid.clone())
                }
                // For partial messages, tool blocks are usually incomplete
                ContentBlock::ToolUse { id, name, input } => {
                    let payload = ToolCallDeltaPayload {
                        call_id: id.clone(),
                        tool_name: name.clone(),
                        arguments_delta: serde_json::to_string(input).unwrap_or_default(),
                    };
                    StreamEvent::new(
                        session_id.clone(),
                        "claude_code".to_string(),
                        stream_id,
                        seq,
                        StreamEventType::ToolCallDelta,
                        serde_json::to_value(&payload).ok()?,
                    )
                    .with_message_id(message_uuid.clone())
                }
                // Tool results in partial messages are rare but handle them
                ContentBlock::ToolResult { .. } => continue,
            };

            events.push(event);
        }

        if events.is_empty() {
            None
        } else {
            Some(events)
        }
    }

    /// Emit a status event for session state changes.
    pub fn emit_status(&self, session_id: &str, state: AgentProcessingState) -> StreamEvent {
        let payload = StatusPayload {
            state,
            detail: None,
        };

        let stream_id = format!("status_{}", session_id);

        StreamEvent::new(
            session_id.to_string(),
            "claude_code".to_string(),
            stream_id,
            0,
            StreamEventType::Status,
            serde_json::to_value(&payload).unwrap_or_default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapt_text_content_block() {
        let adapter = ClaudeCodeAdapter::new();
        let raw = serde_json::json!({
            "type": "assistant",
            "uuid": "msg_123",
            "session_id": "sess_abc",
            "message": {
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "Hello, world!"}
                ]
            },
            "parent_tool_use_id": null
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);

        let event = &events[0];
        assert_eq!(event.event_type, StreamEventType::TextDelta);
        assert_eq!(event.harness, "claude_code");
        assert!(event.stream_id.starts_with("stream_msg_123"));
        assert_eq!(event.seq, 0);
        assert_eq!(event.session_id, "sess_abc");
    }

    #[test]
    fn adapt_thinking_content_block() {
        let adapter = ClaudeCodeAdapter::new();
        let raw = serde_json::json!({
            "type": "assistant",
            "uuid": "msg_123",
            "session_id": "sess_abc",
            "message": {
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "Let me think about this..."}
                ]
            },
            "parent_tool_use_id": null
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, StreamEventType::ThinkingDelta);
    }

    #[test]
    fn adapt_tool_use_content_block() {
        let adapter = ClaudeCodeAdapter::new();
        let raw = serde_json::json!({
            "type": "assistant",
            "uuid": "msg_123",
            "session_id": "sess_abc",
            "message": {
                "role": "assistant",
                "content": [
                    {
                        "type": "tool_use",
                        "id": "tool_1",
                        "name": "read_file",
                        "input": {"path": "/tmp/test.txt"}
                    }
                ]
            },
            "parent_tool_use_id": null
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, StreamEventType::ToolCallDelta);
    }

    #[test]
    fn adapt_tool_result_content_block() {
        let adapter = ClaudeCodeAdapter::new();
        let raw = serde_json::json!({
            "type": "assistant",
            "uuid": "msg_123",
            "session_id": "sess_abc",
            "message": {
                "role": "assistant",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "tool_1",
                        "content": "File contents here",
                        "is_error": false
                    }
                ]
            },
            "parent_tool_use_id": null
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, StreamEventType::ToolCallCompleted);
    }

    #[test]
    fn adapt_result_message_success() {
        let adapter = ClaudeCodeAdapter::new();
        let raw = serde_json::json!({
            "type": "result",
            "subtype": "success",
            "session_id": "sess_abc",
            "duration_ms": 5000,
            "is_error": false,
            "num_turns": 3,
            "result": "Task completed successfully",
            "total_cost_usd": 0.05,
            "usage": {
                "input_tokens": 100,
                "output_tokens": 50,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0
            }
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, StreamEventType::Completed);
    }

    #[test]
    fn adapt_result_message_error() {
        let adapter = ClaudeCodeAdapter::new();
        let raw = serde_json::json!({
            "type": "result",
            "subtype": "error_during_execution",
            "session_id": "sess_abc",
            "duration_ms": 1000,
            "is_error": true,
            "num_turns": 1,
            "result": "Tool execution failed",
            "total_cost_usd": 0.01,
            "usage": null
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 2);
        // Error event should come first
        assert_eq!(events[0].event_type, StreamEventType::Error);
        assert_eq!(events[1].event_type, StreamEventType::Completed);
    }

    #[test]
    fn adapt_partial_assistant_message() {
        let adapter = ClaudeCodeAdapter::new();
        let raw = serde_json::json!({
            "type": "partial_assistant",
            "uuid": "msg_123",
            "session_id": "sess_abc",
            "message": {
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "Partial response..."}
                ]
            }
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, StreamEventType::TextDelta);
    }

    #[test]
    fn seq_increments_per_stream() {
        let adapter = ClaudeCodeAdapter::new();

        // First message
        let raw1 = serde_json::json!({
            "type": "assistant",
            "uuid": "msg_1",
            "session_id": "sess_abc",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "First"}]
            },
            "parent_tool_use_id": null
        });

        // Second message with same UUID (same stream)
        let raw2 = serde_json::json!({
            "type": "assistant",
            "uuid": "msg_1",
            "session_id": "sess_abc",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "Second"}]
            },
            "parent_tool_use_id": null
        });

        let events1 = adapter.adapt(&raw1).unwrap();
        let events2 = adapter.adapt(&raw2).unwrap();

        assert_eq!(events1[0].seq, 0);
        assert_eq!(events2[0].seq, 1);
        assert_eq!(events1[0].stream_id, events2[0].stream_id);
    }

    #[test]
    fn deduplicates_tool_completed_events() {
        let adapter = ClaudeCodeAdapter::new();
        let raw = serde_json::json!({
            "type": "assistant",
            "uuid": "msg_123",
            "session_id": "sess_abc",
            "message": {
                "role": "assistant",
                "content": [
                    {
                        "type": "tool_result",
                        "tool_use_id": "tool_1",
                        "content": "Result",
                        "is_error": false
                    }
                ]
            },
            "parent_tool_use_id": null
        });

        // First call should produce event
        let events1 = adapter.adapt(&raw);
        assert!(events1.is_some());
        assert_eq!(events1.unwrap().len(), 1);

        // Second call with same tool_use_id should produce no events
        let events2 = adapter.adapt(&raw);
        assert!(events2.is_none());
    }

    #[test]
    fn ignores_user_and_system_messages() {
        let adapter = ClaudeCodeAdapter::new();

        let user = serde_json::json!({
            "type": "user",
            "uuid": "msg_123",
            "session_id": "sess_abc"
        });

        let system = serde_json::json!({
            "type": "system",
            "session_id": "sess_abc"
        });

        assert!(adapter.adapt(&user).is_none());
        assert!(adapter.adapt(&system).is_none());
    }

    #[test]
    fn emit_status_creates_valid_event() {
        let adapter = ClaudeCodeAdapter::new();
        let event = adapter.emit_status("sess_abc", AgentProcessingState::Processing);

        assert_eq!(event.event_type, StreamEventType::Status);
        assert_eq!(event.harness, "claude_code");
        assert_eq!(event.session_id, "sess_abc");
        assert!(event.stream_id.starts_with("status_"));
    }

    #[test]
    fn multiple_content_blocks_produce_multiple_events() {
        let adapter = ClaudeCodeAdapter::new();
        let raw = serde_json::json!({
            "type": "assistant",
            "uuid": "msg_123",
            "session_id": "sess_abc",
            "message": {
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "First"},
                    {"type": "thinking", "thinking": "Reasoning..."},
                    {"type": "text", "text": "Second"}
                ]
            },
            "parent_tool_use_id": null
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].event_type, StreamEventType::TextDelta);
        assert_eq!(events[1].event_type, StreamEventType::ThinkingDelta);
        assert_eq!(events[2].event_type, StreamEventType::TextDelta);

        // All should have incrementing seq
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
        assert_eq!(events[2].seq, 2);
    }
}
