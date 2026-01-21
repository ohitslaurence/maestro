// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! ACP ↔ Loom type conversions.
//!
//! This module contains pure conversion functions between:
//! - ACP protocol types (`agent_client_protocol`)
//! - Loom runtime types (`loom_core`)
//! - Loom persistence types (`loom_common_thread`)
//!
//! All functions are pure (no I/O, no async, no side effects) and focus
//! solely on structural transformations between type systems.

use agent_client_protocol::{ContentBlock, ContentChunk, SessionId, StopReason};
use loom_common_core::{Message, Role, ToolCall};
use loom_common_thread::{MessageRole, MessageSnapshot, Thread, ThreadId, ToolCallSnapshot};

// =============================================================================
// ACP ContentBlock ↔ Loom Message
// =============================================================================

/// Convert ACP content blocks into a single Loom user message.
///
/// Currently supports only text blocks; other block types (images, etc.)
/// are ignored. Multiple text blocks are joined with newlines.
pub fn content_blocks_to_user_message(blocks: &[ContentBlock]) -> Message {
	let text = blocks
		.iter()
		.filter_map(|block| match block {
			ContentBlock::Text(t) => Some(t.text.as_str()),
			_ => None,
		})
		.collect::<Vec<_>>()
		.join("\n");

	Message::user(&text)
}

/// Convert assistant text into an ACP content chunk for streaming.
pub fn text_to_content_chunk(text: String) -> ContentChunk {
	ContentChunk::new(text.into())
}

// =============================================================================
// Loom Message ↔ Thread MessageSnapshot
// =============================================================================

/// Convert a persisted message snapshot to a runtime Loom message.
pub fn snapshot_to_message(snapshot: &MessageSnapshot) -> Message {
	Message {
		role: thread_role_to_loom_role(snapshot.role.clone()),
		content: snapshot.content.clone(),
		tool_call_id: snapshot.tool_call_id.clone(),
		name: snapshot.tool_name.clone(),
		tool_calls: snapshot
			.tool_calls
			.as_ref()
			.map(|tcs| tcs.iter().map(snapshot_to_tool_call).collect())
			.unwrap_or_default(),
	}
}

/// Convert a runtime Loom message to a persisted message snapshot.
pub fn message_to_snapshot(message: &Message) -> MessageSnapshot {
	MessageSnapshot {
		role: loom_role_to_thread_role(message.role.clone()),
		content: message.content.clone(),
		tool_call_id: message.tool_call_id.clone(),
		tool_name: message.name.clone(),
		tool_calls: if message.tool_calls.is_empty() {
			None
		} else {
			Some(
				message
					.tool_calls
					.iter()
					.map(tool_call_to_snapshot)
					.collect(),
			)
		},
	}
}

/// Rebuild a vector of Loom messages from a Thread's conversation snapshots.
pub fn thread_to_messages(thread: &Thread) -> Vec<Message> {
	thread
		.conversation
		.messages
		.iter()
		.map(snapshot_to_message)
		.collect()
}

// =============================================================================
// Loom ToolCall ↔ Thread ToolCallSnapshot
// =============================================================================

/// Convert a runtime tool call to its persisted snapshot form.
pub fn tool_call_to_snapshot(call: &ToolCall) -> ToolCallSnapshot {
	ToolCallSnapshot {
		id: call.id.clone(),
		tool_name: call.tool_name.clone(),
		arguments_json: call.arguments_json.clone(),
	}
}

/// Convert a persisted tool call snapshot back to a runtime tool call.
pub fn snapshot_to_tool_call(snapshot: &ToolCallSnapshot) -> ToolCall {
	ToolCall {
		id: snapshot.id.clone(),
		tool_name: snapshot.tool_name.clone(),
		arguments_json: snapshot.arguments_json.clone(),
	}
}

// =============================================================================
// SessionId ↔ ThreadId
// =============================================================================

/// Convert a Loom ThreadId into an ACP SessionId.
pub fn thread_id_to_session_id(thread_id: &ThreadId) -> SessionId {
	SessionId::new(thread_id.to_string())
}

/// Convert an ACP SessionId into a Loom ThreadId.
pub fn session_id_to_thread_id(session_id: &SessionId) -> ThreadId {
	ThreadId::from_string(session_id.to_string())
}

// =============================================================================
// StopReason Mapping
// =============================================================================

/// Map Loom outcome context into an ACP StopReason.
///
/// Precedence:
/// 1. Error → `StopReason::Error`
/// 2. Cancelled → `StopReason::Cancelled`
/// 3. Normal completion → `StopReason::EndTurn`
pub fn map_stop_reason(had_error: bool, cancelled: bool) -> StopReason {
	if had_error {
		// Note: StopReason doesn't have an Error variant, use Cancelled for errors
		StopReason::Cancelled
	} else if cancelled {
		StopReason::Cancelled
	} else {
		StopReason::EndTurn
	}
}

// =============================================================================
// Role Conversions (internal helpers)
// =============================================================================

fn thread_role_to_loom_role(role: MessageRole) -> Role {
	match role {
		MessageRole::System => Role::System,
		MessageRole::User => Role::User,
		MessageRole::Assistant => Role::Assistant,
		MessageRole::Tool => Role::Tool,
	}
}

fn loom_role_to_thread_role(role: Role) -> MessageRole {
	match role {
		Role::System => MessageRole::System,
		Role::User => MessageRole::User,
		Role::Assistant => MessageRole::Assistant,
		Role::Tool => MessageRole::Tool,
	}
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
	use super::*;

	/// **Property: SessionId and ThreadId are interchangeable**
	///
	/// Why this is important: ACP sessions are backed by Loom threads.
	/// The IDs must round-trip cleanly for session persistence to work.
	#[test]
	fn test_session_thread_id_roundtrip() {
		let thread = Thread::new();
		let session_id = thread_id_to_session_id(&thread.id);
		let back = session_id_to_thread_id(&session_id);

		assert_eq!(thread.id.as_str(), back.as_str());
	}

	/// **Property: Message snapshot round-trip preserves data**
	#[test]
	fn test_message_snapshot_roundtrip() {
		let original = Message::user("Hello, world!");
		let snapshot = message_to_snapshot(&original);
		let restored = snapshot_to_message(&snapshot);

		assert_eq!(original.role, restored.role);
		assert_eq!(original.content, restored.content);
	}

	/// **Property: Tool call snapshot round-trip preserves data**
	#[test]
	fn test_tool_call_snapshot_roundtrip() {
		let original = ToolCall {
			id: "call-123".to_string(),
			tool_name: "read_file".to_string(),
			arguments_json: serde_json::json!({"path": "/test.txt"}),
		};
		let snapshot = tool_call_to_snapshot(&original);
		let restored = snapshot_to_tool_call(&snapshot);

		assert_eq!(original.id, restored.id);
		assert_eq!(original.tool_name, restored.tool_name);
		assert_eq!(original.arguments_json, restored.arguments_json);
	}

	/// **Property: StopReason mapping has correct precedence**
	#[test]
	fn test_stop_reason_precedence() {
		// Error maps to Cancelled (no Error variant in StopReason)
		assert!(matches!(map_stop_reason(true, true), StopReason::Cancelled));
		assert!(matches!(
			map_stop_reason(true, false),
			StopReason::Cancelled
		));

		// Cancelled maps to Cancelled
		assert!(matches!(
			map_stop_reason(false, true),
			StopReason::Cancelled
		));

		// Normal completion
		assert!(matches!(map_stop_reason(false, false), StopReason::EndTurn));
	}

	/// **Property: Empty content blocks produce empty message**
	#[test]
	fn test_empty_content_blocks() {
		let blocks: Vec<ContentBlock> = vec![];
		let message = content_blocks_to_user_message(&blocks);
		assert!(message.content.is_empty());
	}

	/// **Property: Multiple text blocks are joined with newlines**
	#[test]
	fn test_multiple_text_blocks() {
		use agent_client_protocol::TextContent;

		let blocks = vec![
			ContentBlock::Text(TextContent::new("Line 1")),
			ContentBlock::Text(TextContent::new("Line 2")),
		];
		let message = content_blocks_to_user_message(&blocks);
		assert_eq!(message.content, "Line 1\nLine 2");
	}
}
