// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! SSE stream parsing for proxy LLM responses.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::Stream;
use pin_project_lite::pin_project;
use tracing::{debug, trace, warn};

use loom_common_core::{LlmError, LlmEvent};

use crate::types::LlmStreamEvent;

pin_project! {
		/// A stream that parses SSE events from an HTTP response into LLM events.
		///
		/// Expects SSE format with "event: llm" and "data: {...}" lines.
		pub struct ProxyLlmStream {
				#[pin]
				inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
				buffer: String,
		}
}

impl ProxyLlmStream {
	/// Creates a new proxy stream from a byte stream.
	pub fn new(inner: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>) -> Self {
		trace!("creating new ProxyLlmStream");
		Self {
			inner,
			buffer: String::new(),
		}
	}

	/// Parses a single SSE event block into an LlmEvent.
	fn parse_event_block(&self, block: &str) -> Option<LlmEvent> {
		let mut event_type: Option<&str> = None;
		let mut data: Option<&str> = None;

		for line in block.lines() {
			if let Some(value) = line.strip_prefix("event:") {
				event_type = Some(value.trim());
			} else if let Some(value) = line.strip_prefix("data:") {
				data = Some(value.trim());
			}
		}

		let event_type = event_type?;
		let data = data?;

		if event_type != "llm" {
			debug!(event_type = %event_type, "ignoring non-llm SSE event");
			return None;
		}

		match serde_json::from_str::<LlmStreamEvent>(data) {
			Ok(stream_event) => {
				trace!(event = ?stream_event, "parsed LlmStreamEvent");
				Some(self.convert_stream_event(stream_event))
			}
			Err(e) => {
				warn!(error = %e, data = %data, "failed to parse SSE data as LlmStreamEvent");
				Some(LlmEvent::Error(LlmError::InvalidResponse(format!(
					"failed to parse SSE event: {e}"
				))))
			}
		}
	}

	/// Converts a wire format LlmStreamEvent to a core LlmEvent.
	///
	/// # Note on ServerQuery Events
	///
	/// Server queries are converted to `Error` events since `LlmEvent` does not
	/// have a dedicated ServerQuery variant. The client layer (outside of the proxy)
	/// must handle server queries that come through the SSE stream before they
	/// reach this proxy layer. This preserves the separation between the proxy's
	/// concerns (wire format conversion) and the server's concerns (query management).
	fn convert_stream_event(&self, event: LlmStreamEvent) -> LlmEvent {
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
			LlmStreamEvent::ServerQuery(query) => {
				tracing::debug!(
						query_id = %query.id,
						"received server_query in SSE stream; handling at client layer"
				);
				LlmEvent::Error(LlmError::Api(format!(
					"server_query events should be handled at client layer: {}",
					query.id
				)))
			}
			LlmStreamEvent::Completed { response } => LlmEvent::Completed(response.into()),
			LlmStreamEvent::Error { message } => LlmEvent::Error(LlmError::Api(message)),
		}
	}
}

impl Stream for ProxyLlmStream {
	type Item = LlmEvent;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let mut this = self.project();

		loop {
			while let Some(end_idx) = this.buffer.find("\n\n") {
				let event_block = this.buffer[..end_idx].to_string();
				*this.buffer = this.buffer[end_idx + 2..].to_string();

				trace!(event_block = %event_block, "parsing buffered SSE event block");

				let dummy_stream = ProxyLlmStream {
					inner: Box::pin(futures::stream::empty()),
					buffer: String::new(),
				};
				if let Some(event) = dummy_stream.parse_event_block(&event_block) {
					return Poll::Ready(Some(event));
				}
			}

			match this.inner.as_mut().poll_next(cx) {
				Poll::Ready(Some(Ok(bytes))) => {
					if let Ok(text) = std::str::from_utf8(&bytes) {
						trace!(bytes_len = bytes.len(), "received SSE chunk");
						this.buffer.push_str(text);
					} else {
						warn!("received non-UTF8 SSE data");
						return Poll::Ready(Some(LlmEvent::Error(LlmError::InvalidResponse(
							"received non-UTF8 data".to_string(),
						))));
					}
				}
				Poll::Ready(Some(Err(e))) => {
					warn!(error = %e, "SSE stream error");
					return Poll::Ready(Some(LlmEvent::Error(LlmError::Http(e.to_string()))));
				}
				Poll::Ready(None) => {
					if !this.buffer.is_empty() {
						debug!(remaining = %this.buffer, "SSE stream ended with unparsed data");
					}
					return Poll::Ready(None);
				}
				Poll::Pending => return Poll::Pending,
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use futures::StreamExt;

	#[tokio::test]
	async fn parses_text_delta_event() {
		let sse_data = b"event: llm\ndata: {\"type\":\"text_delta\",\"content\":\"Hello\"}\n\n";
		let stream =
			futures::stream::once(async { Ok::<_, reqwest::Error>(Bytes::from_static(sse_data)) });
		let mut proxy_stream = ProxyLlmStream::new(Box::pin(stream));

		let event = proxy_stream.next().await;
		assert!(matches!(event, Some(LlmEvent::TextDelta { content }) if content == "Hello"));
	}

	#[tokio::test]
	async fn parses_tool_call_delta_event() {
		let sse_data = b"event: llm\ndata: {\"type\":\"tool_call_delta\",\"call_id\":\"123\",\"tool_name\":\"read\",\"arguments_fragment\":\"{\\\"path\\\":\"}\n\n";
		let stream =
			futures::stream::once(async { Ok::<_, reqwest::Error>(Bytes::from_static(sse_data)) });
		let mut proxy_stream = ProxyLlmStream::new(Box::pin(stream));

		let event = proxy_stream.next().await;
		assert!(matches!(
				event,
				Some(LlmEvent::ToolCallDelta { call_id, tool_name, .. })
				if call_id == "123" && tool_name == "read"
		));
	}

	#[tokio::test]
	async fn parses_error_event() {
		let sse_data = b"event: llm\ndata: {\"type\":\"error\",\"message\":\"rate limited\"}\n\n";
		let stream =
			futures::stream::once(async { Ok::<_, reqwest::Error>(Bytes::from_static(sse_data)) });
		let mut proxy_stream = ProxyLlmStream::new(Box::pin(stream));

		let event = proxy_stream.next().await;
		assert!(matches!(event, Some(LlmEvent::Error(LlmError::Api(msg))) if msg == "rate limited"));
	}

	#[tokio::test]
	async fn handles_chunked_sse_data() {
		let chunk1 = b"event: llm\ndata: {\"type\":\"text_";
		let chunk2 = b"delta\",\"content\":\"Hi\"}\n\n";

		let stream = futures::stream::iter(vec![
			Ok::<_, reqwest::Error>(Bytes::from_static(chunk1)),
			Ok(Bytes::from_static(chunk2)),
		]);
		let mut proxy_stream = ProxyLlmStream::new(Box::pin(stream));

		let event = proxy_stream.next().await;
		assert!(matches!(event, Some(LlmEvent::TextDelta { content }) if content == "Hi"));
	}

	#[tokio::test]
	async fn ignores_non_llm_events() {
		let sse_data = b"event: ping\ndata: {}\n\nevent: llm\ndata: {\"type\":\"text_delta\",\"content\":\"Hi\"}\n\n";
		let stream =
			futures::stream::once(async { Ok::<_, reqwest::Error>(Bytes::from_static(sse_data)) });
		let mut proxy_stream = ProxyLlmStream::new(Box::pin(stream));

		let event = proxy_stream.next().await;
		assert!(matches!(event, Some(LlmEvent::TextDelta { content }) if content == "Hi"));
	}

	#[tokio::test]
	async fn parses_server_query_event() {
		let sse_data = br#"event: llm
data: {"type":"server_query","id":"Q-0123456789abcdef0123456789abcdef","kind":{"type":"read_file","path":"/test.txt"},"sent_at":"2025-01-01T00:00:00Z","timeout_secs":30,"metadata":{}}

"#;
		let stream =
			futures::stream::once(async { Ok::<_, reqwest::Error>(Bytes::from(sse_data.to_vec())) });
		let mut proxy_stream = ProxyLlmStream::new(Box::pin(stream));

		let event = proxy_stream.next().await;
		// Server queries are converted to errors since LlmEvent doesn't have a ServerQuery variant
		assert!(
			matches!(event, Some(LlmEvent::Error(LlmError::Api(msg))) if msg.contains("server_query events should be handled at client layer"))
		);
	}

	mod proptest_streaming {
		use super::*;
		use proptest::collection::vec;
		use proptest::prelude::*;

		fn sse_event_for_text(content: &str) -> String {
			let escaped = content.replace('\\', "\\\\").replace('"', "\\\"");
			format!("event: llm\ndata: {{\"type\":\"text_delta\",\"content\":\"{escaped}\"}}\n\n")
		}

		proptest! {
				/// Validates that text content is preserved through SSE parsing,
				/// ensuring streaming proxy correctly handles arbitrary text.
				#[test]
				fn text_delta_content_preserved(content in "[a-zA-Z0-9 ]{1,50}") {
						let rt = tokio::runtime::Runtime::new().unwrap();
						rt.block_on(async {
								let sse = sse_event_for_text(&content);
								let bytes = Bytes::from(sse);
								let stream = futures::stream::once(async { Ok::<_, reqwest::Error>(bytes) });
								let mut proxy_stream = ProxyLlmStream::new(Box::pin(stream));

								let event = proxy_stream.next().await;
								match event {
										Some(LlmEvent::TextDelta { content: received }) => {
												assert_eq!(content, received);
										}
										other => panic!("expected TextDelta, got {other:?}"),
								}
						});
				}

				/// Validates that multiple SSE events in a single chunk are all parsed,
				/// important for efficient network utilization and correct event ordering.
				#[test]
				fn multiple_events_in_single_chunk(contents in vec("[a-zA-Z0-9]{1,20}", 1..5)) {
						let rt = tokio::runtime::Runtime::new().unwrap();
						rt.block_on(async {
								let sse: String = contents.iter().map(|c| sse_event_for_text(c)).collect();
								let bytes = Bytes::from(sse);
								let stream = futures::stream::once(async { Ok::<_, reqwest::Error>(bytes) });
								let mut proxy_stream = ProxyLlmStream::new(Box::pin(stream));

								for expected_content in &contents {
										let event = proxy_stream.next().await;
										match event {
												Some(LlmEvent::TextDelta { content: received }) => {
														assert_eq!(expected_content, &received);
												}
												other => panic!("expected TextDelta with '{expected_content}', got {other:?}"),
										}
								}

								assert!(proxy_stream.next().await.is_none(), "stream should end after all events");
						});
				}

				/// Validates that SSE events split across chunk boundaries are correctly reassembled,
				/// critical for handling real-world network fragmentation.
				#[test]
				fn events_split_across_chunks(
						content in "[a-zA-Z0-9]{5,20}",
						split_point in 10usize..40,
				) {
						let rt = tokio::runtime::Runtime::new().unwrap();
						rt.block_on(async {
								let sse = sse_event_for_text(&content);
								let split_at = split_point.min(sse.len() - 1).max(1);
								let (chunk1, chunk2) = sse.split_at(split_at);

								let stream = futures::stream::iter(vec![
										Ok::<_, reqwest::Error>(Bytes::from(chunk1.to_string())),
										Ok(Bytes::from(chunk2.to_string())),
								]);
								let mut proxy_stream = ProxyLlmStream::new(Box::pin(stream));

								let event = proxy_stream.next().await;
								match event {
										Some(LlmEvent::TextDelta { content: received }) => {
												assert_eq!(content, received);
										}
										other => panic!("expected TextDelta, got {other:?}"),
								}
						});
				}
		}
	}
}
