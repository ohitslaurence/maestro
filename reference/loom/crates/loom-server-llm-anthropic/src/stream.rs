// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Server-Sent Events (SSE) parser for Anthropic streaming API.

use bytes::Bytes;
use futures::stream::Stream;
use loom_common_core::{LlmError, LlmEvent, LlmResponse, Message, ToolCall, Usage};
use pin_project_lite::pin_project;
use serde::Deserialize;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::{debug, error, trace, warn};

/// SSE event types from Anthropic streaming API.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum StreamEvent {
	#[serde(rename = "message_start")]
	MessageStart { message: MessageStartData },
	#[serde(rename = "content_block_start")]
	ContentBlockStart {
		index: usize,
		content_block: ContentBlock,
	},
	#[serde(rename = "content_block_delta")]
	ContentBlockDelta { index: usize, delta: ContentDelta },
	#[serde(rename = "content_block_stop")]
	ContentBlockStop { index: usize },
	#[serde(rename = "message_delta")]
	MessageDelta {
		delta: MessageDeltaData,
		usage: Option<MessageDeltaUsage>,
	},
	#[serde(rename = "message_stop")]
	MessageStop,
	#[serde(rename = "ping")]
	Ping,
	#[serde(rename = "error")]
	Error { error: StreamErrorData },
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageStartData {
	pub id: String,
	pub model: String,
	pub usage: Option<MessageStartUsage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageStartUsage {
	pub input_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageDeltaUsage {
	pub output_tokens: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
	#[serde(rename = "text")]
	Text { text: String },
	#[serde(rename = "tool_use")]
	ToolUse { id: String, name: String },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentDelta {
	#[serde(rename = "text_delta")]
	TextDelta { text: String },
	#[serde(rename = "input_json_delta")]
	InputJsonDelta { partial_json: String },
}

#[derive(Debug, Clone, Deserialize)]
pub struct MessageDeltaData {
	pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StreamErrorData {
	#[serde(rename = "type")]
	pub error_type: String,
	pub message: String,
}

/// State for tracking the stream during parsing.
#[derive(Debug, Default)]
struct StreamState {
	tool_calls: HashMap<usize, ToolCallBuilder>,
	accumulated_content: String,
	accumulated_tool_calls: Vec<ToolCall>,
	stop_reason: Option<String>,
	input_tokens: u32,
	output_tokens: u32,
}

#[derive(Debug, Default)]
struct ToolCallBuilder {
	id: String,
	name: String,
	arguments_json: String,
}

pin_project! {
		pub struct SseStream<S> {
				#[pin]
				inner: S,
				buffer: String,
				state: StreamState,
				finished: bool,
		}
}

impl<S> SseStream<S> {
	fn new(inner: S) -> Self {
		Self {
			inner,
			buffer: String::new(),
			state: StreamState::default(),
			finished: false,
		}
	}
}

impl<S, E> Stream for SseStream<S>
where
	S: Stream<Item = Result<Bytes, E>>,
	E: std::error::Error,
{
	type Item = LlmEvent;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let mut this = self.project();

		if *this.finished {
			return Poll::Ready(None);
		}

		loop {
			if let Some(event) = try_parse_event(this.buffer, this.state) {
				match event {
					Ok(Some(llm_event)) => {
						if matches!(llm_event, LlmEvent::Completed(_) | LlmEvent::Error(_)) {
							*this.finished = true;
						}
						return Poll::Ready(Some(llm_event));
					}
					Ok(None) => continue,
					Err(e) => {
						*this.finished = true;
						return Poll::Ready(Some(LlmEvent::Error(e)));
					}
				}
			}

			match this.inner.as_mut().poll_next(cx) {
				Poll::Ready(Some(Ok(bytes))) => {
					let chunk = String::from_utf8_lossy(&bytes);
					this.buffer.push_str(&chunk);
					trace!(chunk_len = bytes.len(), "Received SSE chunk");
				}
				Poll::Ready(Some(Err(e))) => {
					error!(error = %e, "Stream error");
					*this.finished = true;
					return Poll::Ready(Some(LlmEvent::Error(LlmError::Http(e.to_string()))));
				}
				Poll::Ready(None) => {
					debug!("Stream ended");
					*this.finished = true;
					return Poll::Ready(None);
				}
				Poll::Pending => return Poll::Pending,
			}
		}
	}
}

fn try_parse_event(
	buffer: &mut String,
	state: &mut StreamState,
) -> Option<Result<Option<LlmEvent>, LlmError>> {
	let event_end = buffer.find("\n\n")?;
	let event_text = buffer[..event_end].to_string();
	buffer.drain(..=event_end + 1);

	let mut data = None;

	for line in event_text.lines() {
		if let Some(value) = line.strip_prefix("data: ") {
			data = Some(value.trim().to_string());
		}
	}

	let Some(data_str) = data else {
		return Some(Ok(None));
	};

	trace!(data = %data_str, "Parsing SSE event");

	let stream_event: StreamEvent = match serde_json::from_str(&data_str) {
		Ok(e) => e,
		Err(e) => {
			warn!(error = %e, data = %data_str, "Failed to parse SSE event");
			return Some(Ok(None));
		}
	};

	Some(process_stream_event(stream_event, state))
}

fn process_stream_event(
	event: StreamEvent,
	state: &mut StreamState,
) -> Result<Option<LlmEvent>, LlmError> {
	match event {
		StreamEvent::MessageStart { message } => {
			debug!(id = %message.id, model = %message.model, "Message started");
			if let Some(usage) = message.usage {
				state.input_tokens = usage.input_tokens;
			}
			Ok(None)
		}
		StreamEvent::ContentBlockStart {
			index,
			content_block,
		} => {
			match content_block {
				ContentBlock::Text { text } => {
					if !text.is_empty() {
						state.accumulated_content.push_str(&text);
						return Ok(Some(LlmEvent::TextDelta { content: text }));
					}
				}
				ContentBlock::ToolUse { id, name } => {
					debug!(index, id = %id, name = %name, "Tool use started");
					state.tool_calls.insert(
						index,
						ToolCallBuilder {
							id,
							name,
							arguments_json: String::new(),
						},
					);
				}
			}
			Ok(None)
		}
		StreamEvent::ContentBlockDelta { index, delta } => match delta {
			ContentDelta::TextDelta { text } => {
				state.accumulated_content.push_str(&text);
				Ok(Some(LlmEvent::TextDelta { content: text }))
			}
			ContentDelta::InputJsonDelta { partial_json } => {
				if let Some(builder) = state.tool_calls.get_mut(&index) {
					builder.arguments_json.push_str(&partial_json);
					Ok(Some(LlmEvent::ToolCallDelta {
						call_id: builder.id.clone(),
						tool_name: builder.name.clone(),
						arguments_fragment: partial_json,
					}))
				} else {
					warn!(index, "Received tool delta for unknown content block");
					Ok(None)
				}
			}
		},
		StreamEvent::ContentBlockStop { index } => {
			if let Some(builder) = state.tool_calls.remove(&index) {
				debug!(
						index,
						id = %builder.id,
						name = %builder.name,
						"Tool call completed"
				);
				let mut arguments: serde_json::Value = serde_json::from_str(&builder.arguments_json)
					.unwrap_or_else(|e| {
						if !builder.arguments_json.is_empty() {
							warn!(
								index,
								id = %builder.id,
								name = %builder.name,
								error = %e,
								raw = %builder.arguments_json,
								"Failed to parse tool arguments JSON, defaulting to empty object"
							);
						}
						serde_json::Value::Object(serde_json::Map::new())
					});
				if arguments.is_null() {
					arguments = serde_json::Value::Object(serde_json::Map::new());
				}
				state.accumulated_tool_calls.push(ToolCall {
					id: builder.id,
					tool_name: builder.name,
					arguments_json: arguments,
				});
			}
			Ok(None)
		}
		StreamEvent::MessageDelta { delta, usage } => {
			if let Some(reason) = delta.stop_reason {
				debug!(stop_reason = %reason, "Message delta with stop reason");
				state.stop_reason = Some(reason);
			}
			if let Some(u) = usage {
				state.output_tokens = u.output_tokens;
			}
			Ok(None)
		}
		StreamEvent::MessageStop => {
			debug!("Message completed");
			let response = LlmResponse {
				message: Message::assistant(std::mem::take(&mut state.accumulated_content)),
				tool_calls: std::mem::take(&mut state.accumulated_tool_calls),
				finish_reason: state.stop_reason.take(),
				usage: Some(Usage {
					input_tokens: state.input_tokens,
					output_tokens: state.output_tokens,
				}),
			};
			Ok(Some(LlmEvent::Completed(response)))
		}
		StreamEvent::Ping => {
			trace!("Received ping");
			Ok(None)
		}
		StreamEvent::Error { error } => {
			error!(
					error_type = %error.error_type,
					message = %error.message,
					"Stream error from API"
			);
			Err(LlmError::Api(error.message))
		}
	}
}

pub fn parse_sse_stream<S, E>(stream: S) -> impl Stream<Item = LlmEvent>
where
	S: Stream<Item = Result<Bytes, E>>,
	E: std::error::Error,
{
	SseStream::new(stream)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_text_delta_event() {
		let json =
			r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
		let event: StreamEvent = serde_json::from_str(json).unwrap();

		let mut state = StreamState::default();
		let result = process_stream_event(event, &mut state).unwrap();

		assert!(matches!(
				result,
				Some(LlmEvent::TextDelta { content }) if content == "Hello"
		));
	}

	#[test]
	fn test_parse_tool_use_start() {
		let json = r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tool_123","name":"get_weather"}}"#;
		let event: StreamEvent = serde_json::from_str(json).unwrap();

		let mut state = StreamState::default();
		let _ = process_stream_event(event, &mut state).unwrap();

		assert!(state.tool_calls.contains_key(&1));
		assert_eq!(state.tool_calls[&1].id, "tool_123");
		assert_eq!(state.tool_calls[&1].name, "get_weather");
	}

	#[test]
	fn test_parse_message_stop() {
		let json = r#"{"type":"message_stop"}"#;
		let event: StreamEvent = serde_json::from_str(json).unwrap();

		let mut state = StreamState::default();
		let result = process_stream_event(event, &mut state).unwrap();

		assert!(matches!(result, Some(LlmEvent::Completed(_))));
	}

	#[test]
	fn test_tool_call_with_no_arguments_defaults_to_empty_object() {
		let mut state = StreamState::default();

		let start_json = r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"tool_123","name":"list_files"}}"#;
		let start_event: StreamEvent = serde_json::from_str(start_json).unwrap();
		process_stream_event(start_event, &mut state).unwrap();

		let stop_json = r#"{"type":"content_block_stop","index":0}"#;
		let stop_event: StreamEvent = serde_json::from_str(stop_json).unwrap();
		process_stream_event(stop_event, &mut state).unwrap();

		assert_eq!(state.accumulated_tool_calls.len(), 1);
		assert!(
			state.accumulated_tool_calls[0].arguments_json.is_object(),
			"Tool arguments should be an object, not null"
		);
		assert_eq!(
			state.accumulated_tool_calls[0].arguments_json,
			serde_json::json!({})
		);
	}
}
