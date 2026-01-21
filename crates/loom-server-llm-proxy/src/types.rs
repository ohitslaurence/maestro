// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Wire format types for LLM proxy communication.

use loom_common_core::{server_query::ServerQuery, Message, ToolCall, Usage};
use serde::{Deserialize, Serialize};

/// Wire format for LLM streaming events sent over SSE.
///
/// # Event Types
///
/// - `TextDelta`: Incremental text content from the assistant
/// - `ToolCallDelta`: Incremental tool call argument data
/// - `ServerQuery`: A query sent from server to client (structured as SSE event with `event: llm`)
/// - `Completed`: The completion has finished successfully
/// - `Error`: An error occurred during streaming
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LlmStreamEvent {
	/// Incremental text content from the assistant.
	TextDelta { content: String },
	/// Incremental tool call data.
	ToolCallDelta {
		call_id: String,
		tool_name: String,
		arguments_fragment: String,
	},
	/// A query sent from server to client during streaming.
	/// The query must be processed by the client and a response sent back via
	/// the query response endpoint.
	ServerQuery(ServerQuery),
	/// The completion has finished successfully.
	Completed { response: LlmProxyResponse },
	/// An error occurred during streaming.
	Error { message: String },
}

/// Wire format for LLM response, serializable for proxy communication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProxyResponse {
	pub message: Message,
	pub tool_calls: Vec<ToolCall>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub usage: Option<Usage>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub finish_reason: Option<String>,
}

impl From<LlmProxyResponse> for loom_common_core::LlmResponse {
	fn from(proxy: LlmProxyResponse) -> Self {
		Self {
			message: proxy.message,
			tool_calls: proxy.tool_calls,
			usage: proxy.usage,
			finish_reason: proxy.finish_reason,
		}
	}
}

impl From<loom_common_core::LlmResponse> for LlmProxyResponse {
	fn from(response: loom_common_core::LlmResponse) -> Self {
		Self {
			message: response.message,
			tool_calls: response.tool_calls,
			usage: response.usage,
			finish_reason: response.finish_reason,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_common_core::Role;
	use proptest::prelude::*;

	proptest! {
			/// Validates that LlmStreamEvent text_delta events serialize correctly,
			/// ensuring SSE communication preserves content integrity.
			#[test]
			fn text_delta_serialization_roundtrip(content in "[a-zA-Z0-9 ]{0,100}") {
					let event = LlmStreamEvent::TextDelta { content: content.clone() };

					let json = serde_json::to_string(&event).expect("serialization should succeed");
					let deserialized: LlmStreamEvent = serde_json::from_str(&json)
							.expect("deserialization should succeed");

					match deserialized {
							LlmStreamEvent::TextDelta { content: deserialized_content } => {
									prop_assert_eq!(content, deserialized_content);
							}
							_ => prop_assert!(false, "expected TextDelta variant"),
					}
			}

			/// Validates that LlmStreamEvent tool_call_delta events serialize correctly,
			/// ensuring tool invocation data is preserved during proxy communication.
			#[test]
			fn tool_call_delta_serialization_roundtrip(
					call_id in "[a-zA-Z0-9_-]{1,32}",
					tool_name in "[a-zA-Z_][a-zA-Z0-9_]{0,30}",
					arguments_fragment in "\\{[a-zA-Z0-9:,\" ]{0,50}\\}?",
			) {
					let event = LlmStreamEvent::ToolCallDelta {
							call_id: call_id.clone(),
							tool_name: tool_name.clone(),
							arguments_fragment: arguments_fragment.clone(),
					};

					let json = serde_json::to_string(&event).expect("serialization should succeed");
					let deserialized: LlmStreamEvent = serde_json::from_str(&json)
							.expect("deserialization should succeed");

					match deserialized {
							LlmStreamEvent::ToolCallDelta {
									call_id: d_call_id,
									tool_name: d_tool_name,
									arguments_fragment: d_args,
							} => {
									prop_assert_eq!(call_id, d_call_id);
									prop_assert_eq!(tool_name, d_tool_name);
									prop_assert_eq!(arguments_fragment, d_args);
							}
							_ => prop_assert!(false, "expected ToolCallDelta variant"),
					}
			}

			/// Validates that LlmProxyResponse serialization preserves all fields,
			/// critical for accurate response forwarding through the proxy.
			#[test]
			fn proxy_response_serialization_roundtrip(
					content in "[a-zA-Z0-9 ]{0,100}",
					input_tokens in 0u32..1_000_000,
					output_tokens in 0u32..1_000_000,
			) {
					let response = LlmProxyResponse {
							message: Message {
									role: Role::Assistant,
									content,
									tool_call_id: None,
									name: None,
									tool_calls: Vec::new(),
							},
							tool_calls: vec![],
							usage: Some(Usage { input_tokens, output_tokens }),
							finish_reason: Some("stop".to_string()),
					};

					let json = serde_json::to_string(&response).expect("serialization should succeed");
					let deserialized: LlmProxyResponse = serde_json::from_str(&json)
							.expect("deserialization should succeed");

					prop_assert_eq!(response.message.content, deserialized.message.content);
					prop_assert_eq!(response.usage.as_ref().unwrap().input_tokens, deserialized.usage.as_ref().unwrap().input_tokens);
					prop_assert_eq!(response.usage.as_ref().unwrap().output_tokens, deserialized.usage.as_ref().unwrap().output_tokens);
			}

			/// Validates that LlmStreamEvent server_query events serialize correctly.
			/// **Why Important**: Server queries must be accurately transmitted through SSE,
			/// as they are critical for server-client communication during streaming.
			#[test]
			fn server_query_serialization_roundtrip(query_id in "Q-[a-f0-9]{32}") {
					use loom_common_core::server_query::ServerQueryKind;

					let query = ServerQuery {
							id: query_id,
							kind: ServerQueryKind::ReadFile { path: "/test.txt".to_string() },
							sent_at: "2025-01-01T00:00:00Z".to_string(),
							timeout_secs: 30,
							metadata: serde_json::json!({}),
					};

					let event = LlmStreamEvent::ServerQuery(query.clone());
					let json = serde_json::to_string(&event).expect("serialization should succeed");
					let deserialized: LlmStreamEvent = serde_json::from_str(&json)
							.expect("deserialization should succeed");

					match deserialized {
							LlmStreamEvent::ServerQuery(deserialized_query) => {
									prop_assert_eq!(query.id, deserialized_query.id);
									prop_assert_eq!(query.timeout_secs, deserialized_query.timeout_secs);
							}
							_ => prop_assert!(false, "expected ServerQuery variant"),
					}
			}
	}
}
