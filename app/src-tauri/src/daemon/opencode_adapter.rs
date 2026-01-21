//! OpenCode to StreamEvent adapter (streaming-event-schema.md §2, §5)
//!
//! Maps OpenCode daemon events to the unified StreamEvent schema.
//! Maintains stream ordering state (streamId + seq) per workspace.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Deserialize;
use serde_json::Value;

use crate::sessions::{
    AgentProcessingState, ErrorPayload, StatusPayload, StreamErrorCode, StreamEvent,
    StreamEventType, TextDeltaPayload, ThinkingDeltaPayload, ToolCallCompletedPayload,
    ToolCallDeltaPayload, ToolCallStatus,
};

// ============================================================================
// OpenCode Event Structures
// ============================================================================

/// Raw OpenCode event from daemon.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeDaemonEvent {
    pub workspace_id: String,
    #[allow(dead_code)]
    pub event_type: String,
    pub event: OpenCodeInnerEvent,
}

/// Inner event with type discriminator.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCodeInnerEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub properties: Option<Value>,
}

/// Part data from message.part.updated events.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PartData {
    pub id: String,
    #[serde(rename = "messageID")]
    pub message_id: String,
    #[serde(rename = "sessionID")]
    pub session_id: String,
    #[serde(rename = "type")]
    pub part_type: String,
    // Text content fields (mutually exclusive based on part_type)
    pub text: Option<String>,
    pub content: Option<String>, // For reasoning parts
    pub output: Option<String>,  // For tool parts
    // Tool-specific fields
    pub tool: Option<String>,
    #[serde(rename = "toolCallID")]
    pub tool_call_id: Option<String>,
    pub input: Option<Value>,
    pub error: Option<String>,
    // Timing
    pub time: Option<PartTime>,
}

#[derive(Debug, Deserialize)]
pub struct PartTime {
    pub start: Option<u64>,
    pub end: Option<u64>,
}

/// Session status from session.status events.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusProps {
    #[serde(rename = "sessionID")]
    pub session_id: Option<String>,
    pub status: Option<SessionStatusType>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatusType {
    #[serde(rename = "type")]
    pub status_type: Option<String>,
}

/// Session error from session.error events.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionErrorProps {
    #[serde(rename = "sessionID")]
    pub session_id: Option<String>,
    pub error: Option<String>,
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
    fn new(message_id: &str) -> Self {
        // Use messageId as streamId since it's stable per assistant response
        Self {
            stream_id: format!("stream_{}", message_id),
            seq: AtomicU64::new(0),
        }
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst)
    }
}

/// Adapter state for a workspace.
/// Tracks active streams by messageId for ordering.
#[derive(Default)]
pub struct WorkspaceStreamState {
    /// Active streams by messageId
    streams: HashMap<String, StreamState>,
    /// Track completed tool calls to avoid duplicate events
    completed_tools: std::collections::HashSet<String>,
}

impl WorkspaceStreamState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create stream state for a message.
    fn get_or_create_stream(&mut self, message_id: &str) -> &StreamState {
        self.streams
            .entry(message_id.to_string())
            .or_insert_with(|| StreamState::new(message_id))
    }

    /// Mark a tool call as completed.
    fn mark_tool_completed(&mut self, tool_call_id: &str) -> bool {
        self.completed_tools.insert(tool_call_id.to_string())
    }

    /// Check if a tool call was already completed.
    fn is_tool_completed(&self, tool_call_id: &str) -> bool {
        self.completed_tools.contains(tool_call_id)
    }
}

/// Global adapter state across workspaces.
#[derive(Default)]
pub struct OpenCodeAdapter {
    workspaces: std::sync::Mutex<HashMap<String, WorkspaceStreamState>>,
}

impl OpenCodeAdapter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create workspace state.
    fn with_workspace<F, R>(&self, workspace_id: &str, f: F) -> R
    where
        F: FnOnce(&mut WorkspaceStreamState) -> R,
    {
        let mut workspaces = self.workspaces.lock().unwrap();
        let state = workspaces
            .entry(workspace_id.to_string())
            .or_insert_with(WorkspaceStreamState::new);
        f(state)
    }

    /// Adapt an OpenCode daemon event to StreamEvent(s).
    ///
    /// Returns None if the event should not be converted (e.g., heartbeats).
    pub fn adapt(&self, raw: &Value) -> Option<Vec<StreamEvent>> {
        let event: OpenCodeDaemonEvent = serde_json::from_value(raw.clone()).ok()?;
        let workspace_id = &event.workspace_id;
        let inner = &event.event;
        let props = inner.properties.as_ref();

        match inner.event_type.as_str() {
            "message.part.updated" => self.adapt_part_updated(workspace_id, props),
            "session.status" => self.adapt_session_status(workspace_id, props),
            "session.error" => self.adapt_session_error(workspace_id, props),
            "session.idle" => self.adapt_session_idle(workspace_id, props),
            // Ignored events (heartbeats, session.created, etc.)
            _ => None,
        }
    }

    /// Adapt message.part.updated to appropriate StreamEvent type.
    fn adapt_part_updated(&self, workspace_id: &str, props: Option<&Value>) -> Option<Vec<StreamEvent>> {
        let props = props?;
        let part: PartData = serde_json::from_value(props.get("part")?.clone()).ok()?;
        let delta = props.get("delta").and_then(|v| v.as_str());

        // Get stream state
        let (stream_id, seq) = self.with_workspace(workspace_id, |ws| {
            let stream = ws.get_or_create_stream(&part.message_id);
            (stream.stream_id.clone(), stream.next_seq())
        });

        let session_id = part.session_id.clone();
        let message_id = part.message_id.clone();

        let event = match part.part_type.as_str() {
            "text" => {
                // Text delta from assistant
                let text = delta.unwrap_or_else(|| part.text.as_deref().unwrap_or(""));
                let payload = TextDeltaPayload {
                    text: text.to_string(),
                    role: "assistant".to_string(),
                };
                StreamEvent::new(
                    session_id,
                    "open_code".to_string(),
                    stream_id,
                    seq,
                    StreamEventType::TextDelta,
                    serde_json::to_value(&payload).ok()?,
                )
                .with_message_id(message_id)
            }
            "reasoning" => {
                // Thinking/reasoning delta
                let text = delta.unwrap_or_else(|| part.content.as_deref().unwrap_or(""));
                let payload = ThinkingDeltaPayload {
                    text: text.to_string(),
                };
                StreamEvent::new(
                    session_id,
                    "open_code".to_string(),
                    stream_id,
                    seq,
                    StreamEventType::ThinkingDelta,
                    serde_json::to_value(&payload).ok()?,
                )
                .with_message_id(message_id)
            }
            "tool" => {
                // Tool call - could be delta or completed
                let tool_name = part.tool.as_deref().unwrap_or("unknown");
                let call_id = part.tool_call_id.as_deref().unwrap_or(&part.id);

                // Check if tool is completed (has output or end time)
                let is_completed = part.output.is_some() || part.time.as_ref().and_then(|t| t.end).is_some();

                if is_completed {
                    // Check if we already emitted completed for this tool
                    let already_completed = self.with_workspace(workspace_id, |ws| {
                        !ws.mark_tool_completed(call_id)
                    });

                    if already_completed {
                        return None; // Skip duplicate completed event
                    }

                    let status = if part.error.is_some() {
                        ToolCallStatus::Failed
                    } else {
                        ToolCallStatus::Completed
                    };

                    let payload = ToolCallCompletedPayload {
                        call_id: call_id.to_string(),
                        tool_name: tool_name.to_string(),
                        arguments: part.input.clone().unwrap_or(Value::Null),
                        output: part.output.clone().unwrap_or_default(),
                        status,
                        error_message: part.error.clone(),
                    };

                    StreamEvent::new(
                        session_id,
                        "open_code".to_string(),
                        stream_id,
                        seq,
                        StreamEventType::ToolCallCompleted,
                        serde_json::to_value(&payload).ok()?,
                    )
                    .with_message_id(message_id)
                } else {
                    // Tool call in progress - emit delta
                    let args_delta = delta.unwrap_or("");
                    let payload = ToolCallDeltaPayload {
                        call_id: call_id.to_string(),
                        tool_name: tool_name.to_string(),
                        arguments_delta: args_delta.to_string(),
                    };

                    StreamEvent::new(
                        session_id,
                        "open_code".to_string(),
                        stream_id,
                        seq,
                        StreamEventType::ToolCallDelta,
                        serde_json::to_value(&payload).ok()?,
                    )
                    .with_message_id(message_id)
                }
            }
            "step-finish" => {
                // Could emit metadata event with token usage, but spec says metadata is optional.
                // For now, skip step-finish as it's not a streaming delta.
                return None;
            }
            _ => return None,
        };

        Some(vec![event])
    }

    /// Adapt session.status to status StreamEvent.
    fn adapt_session_status(&self, workspace_id: &str, props: Option<&Value>) -> Option<Vec<StreamEvent>> {
        let props = props?;
        let status_props: SessionStatusProps = serde_json::from_value(props.clone()).ok()?;

        let session_id = status_props.session_id.unwrap_or_else(|| workspace_id.to_string());
        let status_type = status_props.status.as_ref()?.status_type.as_deref()?;

        let state = match status_type {
            "busy" => AgentProcessingState::Processing,
            "idle" => AgentProcessingState::Idle,
            _ => return None,
        };

        let payload = StatusPayload {
            state,
            detail: None,
        };

        // For status events, use a synthetic stream_id based on session
        let stream_id = format!("status_{}", session_id);

        let event = StreamEvent::new(
            session_id,
            "open_code".to_string(),
            stream_id,
            0, // Status events don't need sequencing
            StreamEventType::Status,
            serde_json::to_value(&payload).ok()?,
        );

        Some(vec![event])
    }

    /// Adapt session.error to error StreamEvent.
    fn adapt_session_error(&self, workspace_id: &str, props: Option<&Value>) -> Option<Vec<StreamEvent>> {
        let props = props?;
        let error_props: SessionErrorProps = serde_json::from_value(props.clone()).ok()?;

        let session_id = error_props.session_id.unwrap_or_else(|| workspace_id.to_string());
        let error_message = error_props.error.unwrap_or_else(|| "Unknown error".to_string());

        let payload = ErrorPayload {
            code: StreamErrorCode::ProviderError,
            message: error_message,
            recoverable: true, // Assume recoverable; user can retry
            details: None,
        };

        let stream_id = format!("error_{}", session_id);

        let event = StreamEvent::new(
            session_id,
            "open_code".to_string(),
            stream_id,
            0,
            StreamEventType::Error,
            serde_json::to_value(&payload).ok()?,
        );

        Some(vec![event])
    }

    /// Adapt session.idle to status StreamEvent.
    fn adapt_session_idle(&self, workspace_id: &str, props: Option<&Value>) -> Option<Vec<StreamEvent>> {
        let session_id = props
            .and_then(|p| p.get("sessionID"))
            .and_then(|v| v.as_str())
            .unwrap_or(workspace_id)
            .to_string();

        let payload = StatusPayload {
            state: AgentProcessingState::Idle,
            detail: None,
        };

        let stream_id = format!("status_{}", session_id);

        let event = StreamEvent::new(
            session_id,
            "open_code".to_string(),
            stream_id,
            0,
            StreamEventType::Status,
            serde_json::to_value(&payload).ok()?,
        );

        Some(vec![event])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapt_text_delta_event() {
        let adapter = OpenCodeAdapter::new();
        let raw = serde_json::json!({
            "workspaceId": "ws_1",
            "eventType": "message.part.updated",
            "event": {
                "type": "message.part.updated",
                "properties": {
                    "part": {
                        "id": "part_1",
                        "messageID": "msg_1",
                        "sessionID": "sess_1",
                        "type": "text",
                        "text": "Hello"
                    },
                    "delta": "Hello"
                }
            }
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);

        let event = &events[0];
        assert_eq!(event.event_type, StreamEventType::TextDelta);
        assert_eq!(event.harness, "open_code");
        assert!(event.stream_id.starts_with("stream_msg_1"));
        assert_eq!(event.seq, 0);
    }

    #[test]
    fn adapt_thinking_delta_event() {
        let adapter = OpenCodeAdapter::new();
        let raw = serde_json::json!({
            "workspaceId": "ws_1",
            "eventType": "message.part.updated",
            "event": {
                "type": "message.part.updated",
                "properties": {
                    "part": {
                        "id": "part_1",
                        "messageID": "msg_1",
                        "sessionID": "sess_1",
                        "type": "reasoning",
                        "content": "Thinking..."
                    },
                    "delta": "Thinking..."
                }
            }
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, StreamEventType::ThinkingDelta);
    }

    #[test]
    fn adapt_tool_call_delta_event() {
        let adapter = OpenCodeAdapter::new();
        let raw = serde_json::json!({
            "workspaceId": "ws_1",
            "eventType": "message.part.updated",
            "event": {
                "type": "message.part.updated",
                "properties": {
                    "part": {
                        "id": "part_1",
                        "messageID": "msg_1",
                        "sessionID": "sess_1",
                        "type": "tool",
                        "tool": "edit_file",
                        "toolCallID": "call_1",
                        "input": {"path": "/tmp/test.txt"},
                        "time": {"start": 1000}
                    }
                }
            }
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, StreamEventType::ToolCallDelta);
    }

    #[test]
    fn adapt_tool_call_completed_event() {
        let adapter = OpenCodeAdapter::new();
        let raw = serde_json::json!({
            "workspaceId": "ws_1",
            "eventType": "message.part.updated",
            "event": {
                "type": "message.part.updated",
                "properties": {
                    "part": {
                        "id": "part_1",
                        "messageID": "msg_1",
                        "sessionID": "sess_1",
                        "type": "tool",
                        "tool": "edit_file",
                        "toolCallID": "call_1",
                        "input": {"path": "/tmp/test.txt"},
                        "output": "ok",
                        "time": {"start": 1000, "end": 2000}
                    }
                }
            }
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, StreamEventType::ToolCallCompleted);
    }

    #[test]
    fn adapt_session_status_busy() {
        let adapter = OpenCodeAdapter::new();
        let raw = serde_json::json!({
            "workspaceId": "ws_1",
            "eventType": "session.status",
            "event": {
                "type": "session.status",
                "properties": {
                    "sessionID": "sess_1",
                    "status": {"type": "busy"}
                }
            }
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, StreamEventType::Status);
    }

    #[test]
    fn adapt_session_error() {
        let adapter = OpenCodeAdapter::new();
        let raw = serde_json::json!({
            "workspaceId": "ws_1",
            "eventType": "session.error",
            "event": {
                "type": "session.error",
                "properties": {
                    "sessionID": "sess_1",
                    "error": "Rate limit exceeded"
                }
            }
        });

        let events = adapter.adapt(&raw).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, StreamEventType::Error);
    }

    #[test]
    fn seq_increments_per_stream() {
        let adapter = OpenCodeAdapter::new();

        // First event
        let raw1 = serde_json::json!({
            "workspaceId": "ws_1",
            "eventType": "message.part.updated",
            "event": {
                "type": "message.part.updated",
                "properties": {
                    "part": {
                        "id": "part_1",
                        "messageID": "msg_1",
                        "sessionID": "sess_1",
                        "type": "text",
                        "text": "Hello"
                    }
                }
            }
        });

        // Second event for same message
        let raw2 = serde_json::json!({
            "workspaceId": "ws_1",
            "eventType": "message.part.updated",
            "event": {
                "type": "message.part.updated",
                "properties": {
                    "part": {
                        "id": "part_2",
                        "messageID": "msg_1",
                        "sessionID": "sess_1",
                        "type": "text",
                        "text": " World"
                    },
                    "delta": " World"
                }
            }
        });

        let events1 = adapter.adapt(&raw1).unwrap();
        let events2 = adapter.adapt(&raw2).unwrap();

        assert_eq!(events1[0].seq, 0);
        assert_eq!(events2[0].seq, 1);
        assert_eq!(events1[0].stream_id, events2[0].stream_id);
    }

    #[test]
    fn ignores_heartbeat_events() {
        let adapter = OpenCodeAdapter::new();
        let raw = serde_json::json!({
            "workspaceId": "ws_1",
            "eventType": "server.heartbeat",
            "event": {
                "type": "server.heartbeat",
                "properties": {}
            }
        });

        let events = adapter.adapt(&raw);
        assert!(events.is_none());
    }

    #[test]
    fn deduplicates_tool_completed_events() {
        let adapter = OpenCodeAdapter::new();
        let raw = serde_json::json!({
            "workspaceId": "ws_1",
            "eventType": "message.part.updated",
            "event": {
                "type": "message.part.updated",
                "properties": {
                    "part": {
                        "id": "part_1",
                        "messageID": "msg_1",
                        "sessionID": "sess_1",
                        "type": "tool",
                        "tool": "edit_file",
                        "toolCallID": "call_1",
                        "input": {},
                        "output": "ok",
                        "time": {"start": 1000, "end": 2000}
                    }
                }
            }
        });

        // First call should produce event
        let events1 = adapter.adapt(&raw);
        assert!(events1.is_some());
        assert_eq!(events1.unwrap().len(), 1);

        // Second call with same tool should be deduplicated
        let events2 = adapter.adapt(&raw);
        assert!(events2.is_none());
    }
}
