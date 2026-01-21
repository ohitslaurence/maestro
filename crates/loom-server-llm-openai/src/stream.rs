// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Streaming SSE parser for OpenAI API responses.

use crate::types::{OpenAIError, OpenAIStreamChunk};
use futures::Stream;
use loom_common_core::{LlmError, LlmEvent, LlmResponse, Message, ToolCall, Usage};
use pin_project_lite::pin_project;
use std::collections::HashMap;
use std::pin::Pin;
use std::task::{Context, Poll};
use tracing::{debug, trace, warn};

pin_project! {
		/// Stream adapter that parses OpenAI SSE events into LlmEvents.
		///
		/// # Purpose
		/// Converts raw Server-Sent Events from the OpenAI streaming API into
		/// structured LlmEvent variants, handling delta accumulation and completion.
		pub struct OpenAIStream<S> {
				#[pin]
				inner: S,
				buffer: String,
				accumulated_content: String,
				accumulated_tool_calls: HashMap<u32, AccumulatedToolCall>,
				finished: bool,
				usage: Option<Usage>,
				finish_reason: Option<String>,
		}
}

#[derive(Debug, Default)]
struct AccumulatedToolCall {
	id: String,
	name: String,
	arguments: String,
}

impl<S> OpenAIStream<S> {
	pub fn new(inner: S) -> Self {
		Self {
			inner,
			buffer: String::new(),
			accumulated_content: String::new(),
			accumulated_tool_calls: HashMap::new(),
			finished: false,
			usage: None,
			finish_reason: None,
		}
	}
}

impl<S, E> Stream for OpenAIStream<S>
where
	S: Stream<Item = Result<bytes::Bytes, E>>,
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
				this.accumulated_tool_calls,
				this.finished,
				this.usage,
				this.finish_reason,
			) {
				return Poll::Ready(Some(event));
			}

			match this.inner.as_mut().poll_next(cx) {
				Poll::Ready(Some(Ok(bytes))) => match std::str::from_utf8(&bytes) {
					Ok(s) => {
						trace!(bytes_len = bytes.len(), "Received SSE data chunk");
						this.buffer.push_str(s);
					}
					Err(e) => {
						warn!(error = %e, "Invalid UTF-8 in stream");
						return Poll::Ready(Some(LlmEvent::Error(LlmError::InvalidResponse(format!(
							"Invalid UTF-8: {e}"
						)))));
					}
				},
				Poll::Ready(Some(Err(e))) => {
					return Poll::Ready(Some(LlmEvent::Error(LlmError::Http(e.to_string()))));
				}
				Poll::Ready(None) => {
					*this.finished = true;
					if !this.accumulated_content.is_empty() || !this.accumulated_tool_calls.is_empty() {
						let response = build_final_response(
							this.accumulated_content,
							this.accumulated_tool_calls,
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

fn try_parse_next_event(
	buffer: &mut String,
	accumulated_content: &mut String,
	accumulated_tool_calls: &mut HashMap<u32, AccumulatedToolCall>,
	finished: &mut bool,
	usage: &mut Option<Usage>,
	finish_reason: &mut Option<String>,
) -> Option<LlmEvent> {
	while let Some(line_end) = buffer.find('\n') {
		let line = buffer[..line_end].trim_end_matches('\r').to_string();
		buffer.drain(..=line_end);

		if line.is_empty() {
			continue;
		}

		if line.starts_with(':') {
			continue;
		}

		if let Some(data) = line.strip_prefix("data: ") {
			let data = data.trim();

			if data == "[DONE]" {
				debug!("Received [DONE] marker");
				*finished = true;
				let response = build_final_response(
					accumulated_content,
					accumulated_tool_calls,
					usage,
					finish_reason,
				);
				return Some(LlmEvent::Completed(response));
			}

			match serde_json::from_str::<OpenAIStreamChunk>(data) {
				Ok(chunk) => {
					if let Some(u) = chunk.usage {
						*usage = Some(Usage {
							input_tokens: u.prompt_tokens,
							output_tokens: u.completion_tokens,
						});
					}

					for choice in chunk.choices {
						if let Some(fr) = &choice.finish_reason {
							*finish_reason = Some(fr.clone());
						}

						if let Some(content) = &choice.delta.content {
							if !content.is_empty() {
								accumulated_content.push_str(content);
								return Some(LlmEvent::TextDelta {
									content: content.clone(),
								});
							}
						}

						if let Some(tool_calls) = &choice.delta.tool_calls {
							for tc_delta in tool_calls {
								let acc = accumulated_tool_calls.entry(tc_delta.index).or_default();

								if let Some(id) = &tc_delta.id {
									acc.id = id.clone();
								}

								if let Some(func) = &tc_delta.function {
									if let Some(name) = &func.name {
										acc.name = name.clone();
									}
									if let Some(args) = &func.arguments {
										acc.arguments.push_str(args);
										return Some(LlmEvent::ToolCallDelta {
											call_id: acc.id.clone(),
											tool_name: acc.name.clone(),
											arguments_fragment: args.clone(),
										});
									}
								}
							}
						}

						if choice.finish_reason.is_some() {
							trace!(finish_reason = ?choice.finish_reason, "Stream finish reason received");
						}
					}
				}
				Err(e) => {
					if let Ok(error_response) = serde_json::from_str::<OpenAIError>(data) {
						warn!(
								error_type = ?error_response.error.error_type,
								message = %error_response.error.message,
								"OpenAI API error in stream"
						);
						return Some(LlmEvent::Error(LlmError::Api(error_response.error.message)));
					}

					warn!(error = %e, data = data, "Failed to parse stream chunk");
				}
			}
		}
	}

	None
}

fn build_final_response(
	accumulated_content: &str,
	accumulated_tool_calls: &HashMap<u32, AccumulatedToolCall>,
	usage: &Option<Usage>,
	finish_reason: &Option<String>,
) -> LlmResponse {
	let tool_calls: Vec<ToolCall> = accumulated_tool_calls
		.values()
		.filter(|tc| !tc.id.is_empty())
		.map(|tc| {
			let mut arguments = serde_json::from_str(&tc.arguments)
				.unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
			if arguments.is_null() {
				arguments = serde_json::Value::Object(serde_json::Map::new());
			}
			ToolCall {
				id: tc.id.clone(),
				tool_name: tc.name.clone(),
				arguments_json: arguments,
			}
		})
		.collect();

	LlmResponse {
		message: Message::assistant(accumulated_content.to_string()),
		tool_calls,
		usage: usage.clone(),
		finish_reason: finish_reason.clone(),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::StreamExt;

	/// Tests that text content deltas are properly accumulated and emitted.
	/// This is important because the streaming API sends content in small chunks
	/// that must be aggregated into a coherent response.
	#[tokio::test]
	async fn test_stream_text_delta() {
		let chunks: Vec<Result<bytes::Bytes, std::io::Error>> = vec![
			Ok(bytes::Bytes::from(
				"data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1234,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
			)),
			Ok(bytes::Bytes::from(
				"data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1234,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" world\"},\"finish_reason\":null}]}\n\n",
			)),
			Ok(bytes::Bytes::from("data: [DONE]\n\n")),
		];

		let inner = futures::stream::iter(chunks);
		let mut stream = OpenAIStream::new(inner);

		let event1 = stream.next().await.unwrap();
		assert!(matches!(event1, LlmEvent::TextDelta { content } if content == "Hello"));

		let event2 = stream.next().await.unwrap();
		assert!(matches!(event2, LlmEvent::TextDelta { content } if content == " world"));

		let event3 = stream.next().await.unwrap();
		if let LlmEvent::Completed(response) = event3 {
			assert_eq!(response.message.content, "Hello world");
		} else {
			panic!("Expected Completed event");
		}
	}

	/// Tests that the [DONE] marker correctly terminates the stream.
	/// This is important because the OpenAI API uses this marker to signal
	/// the end of streaming, and failing to handle it would cause hangs.
	#[tokio::test]
	async fn test_stream_done_marker() {
		let chunks: Vec<Result<bytes::Bytes, std::io::Error>> =
			vec![Ok(bytes::Bytes::from("data: [DONE]\n\n"))];

		let inner = futures::stream::iter(chunks);
		let mut stream = OpenAIStream::new(inner);

		let event = stream.next().await.unwrap();
		assert!(matches!(event, LlmEvent::Completed(_)));

		assert!(stream.next().await.is_none());
	}

	/// Tests that tool call deltas are properly accumulated across multiple chunks.
	/// This is important because tool calls often span multiple SSE events,
	/// with the function arguments being streamed incrementally.
	#[tokio::test]
	async fn test_stream_tool_call_accumulation() {
		let chunks: Vec<Result<bytes::Bytes, std::io::Error>> = vec![
			Ok(bytes::Bytes::from(
				"data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1234,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_123\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\"}}]},\"finish_reason\":null}]}\n\n",
			)),
			Ok(bytes::Bytes::from(
				"data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1234,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"loc\"}}]},\"finish_reason\":null}]}\n\n",
			)),
			Ok(bytes::Bytes::from(
				"data: {\"id\":\"1\",\"object\":\"chat.completion.chunk\",\"created\":1234,\"model\":\"gpt-4\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"ation\\\":\\\"NYC\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
			)),
			Ok(bytes::Bytes::from("data: [DONE]\n\n")),
		];

		let inner = futures::stream::iter(chunks);
		let mut stream = OpenAIStream::new(inner);

		let mut events = Vec::new();
		while let Some(event) = stream.next().await {
			events.push(event);
		}

		let completed = events.last().unwrap();
		if let LlmEvent::Completed(response) = completed {
			assert_eq!(response.tool_calls.len(), 1);
			assert_eq!(response.tool_calls[0].tool_name, "get_weather");
			assert_eq!(
				response.tool_calls[0].arguments_json,
				serde_json::json!({"location": "NYC"})
			);
		} else {
			panic!("Expected Completed event");
		}
	}
}
