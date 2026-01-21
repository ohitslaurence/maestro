// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Streaming parser for Vertex AI API responses.
//!
//! Vertex AI's streamGenerateContent returns newline-delimited JSON objects,
//! not SSE like Anthropic/OpenAI. Each line is a complete `GenerateContentResponse`.

use bytes::Bytes;
use futures::Stream;
use loom_common_core::{LlmError, LlmEvent, LlmResponse, Message, ToolCall, Usage};
use pin_project_lite::pin_project;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::{debug, trace, warn};

use crate::types::{VertexError, VertexPart, VertexResponse};

/// State for accumulating tool calls across stream chunks.
/// Keyed by a stable call_id that is consistent between deltas and final response.
#[derive(Debug, Default)]
struct AccumulatedToolCall {
	call_id: String,
	name: String,
	arguments: String,
}

pin_project! {
		/// Stream adapter that parses Vertex AI streaming responses into LlmEvents.
		///
		/// Vertex streaming returns newline-delimited JSON objects, each being a
		/// complete `GenerateContentResponse` with incremental content.
		pub struct VertexStream<S> {
				#[pin]
				inner: S,
				buffer: String,
				accumulated_content: String,
				tool_calls: HashMap<String, AccumulatedToolCall>,
				tool_call_counter: usize,
				usage: Option<Usage>,
				finish_reason: Option<String>,
				finished: bool,
		}
}

impl<S> VertexStream<S> {
	/// Creates a new Vertex stream parser.
	pub fn new(inner: S) -> Self {
		Self {
			inner,
			buffer: String::new(),
			accumulated_content: String::new(),
			tool_calls: HashMap::new(),
			tool_call_counter: 0,
			usage: None,
			finish_reason: None,
			finished: false,
		}
	}
}

impl<S, E> Stream for VertexStream<S>
where
	S: Stream<Item = Result<Bytes, E>>,
	E: std::fmt::Display,
{
	type Item = LlmEvent;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let mut this = self.project();

		if *this.finished {
			return Poll::Ready(None);
		}

		loop {
			if let Some(event) = try_parse_next_event(
				this.buffer,
				this.accumulated_content,
				this.tool_calls,
				this.tool_call_counter,
				this.usage,
				this.finish_reason,
				this.finished,
			) {
				return Poll::Ready(Some(event));
			}

			match this.inner.as_mut().poll_next(cx) {
				Poll::Ready(Some(Ok(bytes))) => match std::str::from_utf8(&bytes) {
					Ok(s) => {
						trace!(bytes_len = bytes.len(), "Received Vertex stream chunk");
						this.buffer.push_str(s);
					}
					Err(e) => {
						warn!(error = %e, "Invalid UTF-8 in Vertex stream");
						*this.finished = true;
						return Poll::Ready(Some(LlmEvent::Error(LlmError::InvalidResponse(format!(
							"Invalid UTF-8: {e}"
						)))));
					}
				},
				Poll::Ready(Some(Err(e))) => {
					*this.finished = true;
					return Poll::Ready(Some(LlmEvent::Error(LlmError::Http(e.to_string()))));
				}
				Poll::Ready(None) => {
					debug!("Vertex stream ended");
					*this.finished = true;
					if !this.accumulated_content.is_empty() || !this.tool_calls.is_empty() {
						let response = build_final_response(
							this.accumulated_content,
							this.tool_calls,
							this.usage,
							this.finish_reason,
						);
						return Poll::Ready(Some(LlmEvent::Completed(response)));
					}
					return Poll::Ready(None);
				}
				Poll::Pending => return Poll::Pending,
			}
		}
	}
}

/// Attempts to parse the next event from the buffer.
fn try_parse_next_event(
	buffer: &mut String,
	accumulated_content: &mut String,
	tool_calls: &mut HashMap<String, AccumulatedToolCall>,
	tool_call_counter: &mut usize,
	usage: &mut Option<Usage>,
	finish_reason: &mut Option<String>,
	finished: &mut bool,
) -> Option<LlmEvent> {
	while let Some(line_end) = buffer.find('\n') {
		let line = buffer[..line_end].trim().to_string();
		buffer.drain(..=line_end);

		if line.is_empty() {
			continue;
		}

		// Vertex streaming sometimes wraps in array brackets or uses different formats
		// Strip leading/trailing brackets if present
		let json_str = line
			.trim_start_matches('[')
			.trim_start_matches(',')
			.trim_end_matches(']')
			.trim();

		if json_str.is_empty() {
			continue;
		}

		// Try to parse as a normal response chunk
		if let Ok(chunk) = serde_json::from_str::<VertexResponse>(json_str) {
			// Update usage if present
			if let Some(u) = chunk.usage_metadata {
				*usage = Some(Usage {
					input_tokens: u.prompt_token_count,
					output_tokens: u.candidates_token_count.unwrap_or(0),
				});
			}

			// Process candidates
			if let Some(candidate) = chunk.candidates.first() {
				// Track if we have a terminal finish reason
				let is_terminal = if let Some(fr) = &candidate.finish_reason {
					*finish_reason = Some(fr.clone());
					fr == "STOP" || fr == "MAX_TOKENS" || fr == "SAFETY"
				} else {
					false
				};

				// Process content parts first, emit deltas
				for part in &candidate.content.parts {
					match part {
						VertexPart::Text { text } => {
							if !text.is_empty() {
								accumulated_content.push_str(text);
								return Some(LlmEvent::TextDelta {
									content: text.clone(),
								});
							}
						}
						VertexPart::FunctionCall { function_call } => {
							// Generate a stable call_id for this function call.
							// We use the function name as key for now; for multiple
							// calls with same name, each gets a unique ID.
							let call_id = format!("vertex_call_{}", *tool_call_counter);
							*tool_call_counter += 1;

							let acc = tool_calls
								.entry(call_id.clone())
								.or_insert_with(|| AccumulatedToolCall {
									call_id: call_id.clone(),
									name: function_call.name.clone(),
									arguments: String::new(),
								});

							// Serialize args to string for accumulation
							let fragment = serde_json::to_string(&function_call.args).unwrap_or_default();
							acc.arguments.push_str(&fragment);

							return Some(LlmEvent::ToolCallDelta {
								call_id,
								tool_name: function_call.name.clone(),
								arguments_fragment: fragment,
							});
						}
						VertexPart::FunctionResponse { .. } => {
							// Ignore function responses in model output
						}
					}
				}

				// After processing parts, if terminal, emit Completed
				if is_terminal {
					*finished = true;
					let response =
						build_final_response(accumulated_content, tool_calls, usage, finish_reason);
					return Some(LlmEvent::Completed(response));
				}
			}
			continue;
		}

		// Try to parse as an error response
		if let Ok(err) = serde_json::from_str::<VertexError>(json_str) {
			*finished = true;
			return Some(LlmEvent::Error(LlmError::Api(err.error.message)));
		}

		warn!(data = %json_str, "Failed to parse Vertex stream line");
	}
	None
}

/// Builds the final response from accumulated state.
fn build_final_response(
	accumulated_content: &str,
	tool_calls: &HashMap<String, AccumulatedToolCall>,
	usage: &Option<Usage>,
	finish_reason: &Option<String>,
) -> LlmResponse {
	let tool_calls_vec: Vec<ToolCall> = tool_calls
		.values()
		.filter(|tc| !tc.name.is_empty())
		.map(|tc| {
			let mut arguments = serde_json::from_str(&tc.arguments)
				.unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
			if arguments.is_null() {
				arguments = serde_json::Value::Object(serde_json::Map::new());
			}
			ToolCall {
				id: tc.call_id.clone(),
				tool_name: tc.name.clone(),
				arguments_json: arguments,
			}
		})
		.collect();

	LlmResponse {
		message: Message::assistant(accumulated_content.to_string()),
		tool_calls: tool_calls_vec,
		usage: usage.clone(),
		finish_reason: finish_reason.clone(),
	}
}

/// Creates a stream parser for Vertex AI responses.
pub fn parse_vertex_stream<S, E>(stream: S) -> impl Stream<Item = LlmEvent>
where
	S: Stream<Item = Result<Bytes, E>>,
	E: std::fmt::Display,
{
	VertexStream::new(stream)
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::StreamExt;

	/// Verifies text delta events are properly parsed and emitted.
	/// Important for ensuring streaming text responses work correctly.
	#[tokio::test]
	async fn text_delta_parsing() {
		let chunk = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Hello"}]}}]}"#;
		let chunks: Vec<Result<Bytes, std::io::Error>> = vec![Ok(Bytes::from(format!("{chunk}\n")))];

		let inner = futures::stream::iter(chunks);
		let mut stream = VertexStream::new(inner);

		let event = stream.next().await.unwrap();
		assert!(matches!(
				event,
				LlmEvent::TextDelta { content } if content == "Hello"
		));
	}

	/// Verifies completion events are emitted when finish_reason is present.
	/// Important for properly terminating the stream.
	#[tokio::test]
	async fn completion_on_finish_reason() {
		let chunk = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Done"}]},"finishReason":"STOP"}]}"#;
		let chunks: Vec<Result<Bytes, std::io::Error>> = vec![Ok(Bytes::from(format!("{chunk}\n")))];

		let inner = futures::stream::iter(chunks);
		let mut stream = VertexStream::new(inner);

		// First we get the text delta
		let event1 = stream.next().await.unwrap();
		assert!(matches!(event1, LlmEvent::TextDelta { .. }));

		// Then the completed event (since finish_reason triggers it)
		let event2 = stream.next().await.unwrap();
		match event2 {
			LlmEvent::Completed(response) => {
				assert_eq!(response.message.content, "Done");
				assert_eq!(response.finish_reason, Some("STOP".to_string()));
			}
			_ => panic!("Expected Completed event"),
		}
	}

	/// Verifies function calls are properly parsed.
	/// Important for tool use functionality.
	#[tokio::test]
	async fn function_call_parsing() {
		let chunk = r#"{"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"get_weather","args":{"location":"NYC"}}}]}}]}"#;
		let chunks: Vec<Result<Bytes, std::io::Error>> = vec![Ok(Bytes::from(format!("{chunk}\n")))];

		let inner = futures::stream::iter(chunks);
		let mut stream = VertexStream::new(inner);

		let event = stream.next().await.unwrap();
		match event {
			LlmEvent::ToolCallDelta {
				tool_name,
				arguments_fragment,
				..
			} => {
				assert_eq!(tool_name, "get_weather");
				assert!(arguments_fragment.contains("NYC"));
			}
			_ => panic!("Expected ToolCallDelta event"),
		}
	}

	/// Verifies usage metadata is accumulated correctly.
	/// Important for tracking token consumption.
	#[tokio::test]
	async fn usage_metadata_parsing() {
		let chunk = r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"Hi"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":10,"candidatesTokenCount":5,"totalTokenCount":15}}"#;
		let chunks: Vec<Result<Bytes, std::io::Error>> = vec![Ok(Bytes::from(format!("{chunk}\n")))];

		let inner = futures::stream::iter(chunks);
		let mut stream = VertexStream::new(inner);

		// Get the text delta first
		let event1 = stream.next().await.unwrap();
		assert!(matches!(event1, LlmEvent::TextDelta { .. }));

		// Then we get Completed with usage
		let event2 = stream.next().await.unwrap();
		match event2 {
			LlmEvent::Completed(response) => {
				assert_eq!(response.message.content, "Hi");
				let usage = response.usage.expect("usage should be present");
				assert_eq!(usage.input_tokens, 10);
				assert_eq!(usage.output_tokens, 5);
			}
			_ => panic!("Expected Completed event"),
		}
	}

	/// Verifies empty lines are skipped.
	/// Important for handling malformed input gracefully.
	#[tokio::test]
	async fn empty_lines_skipped() {
		let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
			Ok(Bytes::from("\n\n")),
			Ok(Bytes::from(
				r#"{"candidates":[{"content":{"role":"model","parts":[{"text":"X"}]}}]}"#.to_string()
					+ "\n",
			)),
		];

		let inner = futures::stream::iter(chunks);
		let mut stream = VertexStream::new(inner);

		let event = stream.next().await.unwrap();
		assert!(matches!(
				event,
				LlmEvent::TextDelta { content } if content == "X"
		));
	}

	/// Verifies stream ends properly when inner stream ends.
	/// Important for resource cleanup.
	#[tokio::test]
	async fn stream_ends_properly() {
		let chunks: Vec<Result<Bytes, std::io::Error>> = vec![];
		let inner = futures::stream::iter(chunks);
		let mut stream = VertexStream::new(inner);

		let event = stream.next().await;
		assert!(event.is_none());
	}

	/// Verifies tool call IDs are consistent between streaming deltas and final response.
	/// This is critical for downstream code that correlates deltas with final tool calls.
	#[tokio::test]
	async fn tool_call_ids_consistent_between_delta_and_final() {
		let chunk = r#"{"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"get_weather","args":{"location":"NYC"}}}]},"finishReason":"STOP"}]}"#;
		let chunks: Vec<Result<Bytes, std::io::Error>> = vec![Ok(Bytes::from(format!("{chunk}\n")))];

		let inner = futures::stream::iter(chunks);
		let mut stream = VertexStream::new(inner);

		// Get the tool call delta first
		let event1 = stream.next().await.unwrap();
		let delta_call_id = match event1 {
			LlmEvent::ToolCallDelta { call_id, .. } => call_id,
			_ => panic!("Expected ToolCallDelta event"),
		};

		// Then we get Completed
		let event2 = stream.next().await.unwrap();
		let final_call_id = match event2 {
			LlmEvent::Completed(response) => {
				assert_eq!(response.tool_calls.len(), 1);
				response.tool_calls[0].id.clone()
			}
			_ => panic!("Expected Completed event"),
		};

		// IDs must match
		assert_eq!(delta_call_id, final_call_id);
	}

	/// Verifies multiple tool calls with the same name get distinct IDs.
	/// This is important when the model calls the same function multiple times.
	#[tokio::test]
	async fn multiple_tool_calls_get_distinct_ids() {
		let chunk1 = r#"{"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"get_weather","args":{"location":"NYC"}}}]}}]}"#;
		let chunk2 = r#"{"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"get_weather","args":{"location":"LA"}}}]},"finishReason":"STOP"}]}"#;
		let chunks: Vec<Result<Bytes, std::io::Error>> = vec![
			Ok(Bytes::from(format!("{chunk1}\n"))),
			Ok(Bytes::from(format!("{chunk2}\n"))),
		];

		let inner = futures::stream::iter(chunks);
		let mut stream = VertexStream::new(inner);

		// First tool call delta
		let event1 = stream.next().await.unwrap();
		let id1 = match event1 {
			LlmEvent::ToolCallDelta { call_id, .. } => call_id,
			_ => panic!("Expected ToolCallDelta"),
		};

		// Second tool call delta
		let event2 = stream.next().await.unwrap();
		let id2 = match event2 {
			LlmEvent::ToolCallDelta { call_id, .. } => call_id,
			_ => panic!("Expected ToolCallDelta"),
		};

		// IDs must be different
		assert_ne!(id1, id2);

		// Completed event
		let event3 = stream.next().await.unwrap();
		match event3 {
			LlmEvent::Completed(response) => {
				assert_eq!(response.tool_calls.len(), 2);
				// Both tool calls have the same name but different IDs
				assert_eq!(response.tool_calls[0].tool_name, "get_weather");
				assert_eq!(response.tool_calls[1].tool_name, "get_weather");
				let final_ids: Vec<_> = response.tool_calls.iter().map(|tc| &tc.id).collect();
				assert!(final_ids.contains(&&id1));
				assert!(final_ids.contains(&&id2));
			}
			_ => panic!("Expected Completed"),
		}
	}
}
