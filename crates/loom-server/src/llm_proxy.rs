// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! LLM proxy handlers for server-side LLM completion requests.

use axum::{
	extract::State,
	http::StatusCode,
	response::{
		sse::{Event, Sse},
		IntoResponse,
	},
	Json,
};
use loom_common_core::{
	server_query::ServerQuery, LlmError, LlmEvent, LlmRequest, LlmStream, Message, ToolCall, Usage,
};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::ReceiverStream;

use crate::{api::AppState, error::ServerError};

/// Wire format for LLM streaming events sent over SSE.
///
/// # Event Types
///
/// - `TextDelta`: Incremental text content from the assistant
/// - `ToolCallDelta`: Incremental tool call argument data
/// - `ServerQuery`: A query sent from server to client (structured as SSE event
///   with `event: llm`)
/// - `Completed`: The completion has finished successfully
/// - `Error`: An error occurred during streaming
///
/// # SSE Format
///
/// All events are formatted as SSE with `event: llm` header:
/// ```text
/// event: llm
/// data: {"type":"text_delta","content":"..."}
///
/// event: llm
/// data: {"type":"server_query","id":"Q-...","kind":{...},...}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LlmStreamEvent {
	TextDelta {
		content: String,
	},
	ToolCallDelta {
		call_id: String,
		tool_name: String,
		arguments_fragment: String,
	},
	/// A query sent from server to client during streaming.
	/// The query must be processed by the client and a response sent back via
	/// the query response endpoint: `POST
	/// /api/sessions/{session_id}/query-response`
	ServerQuery(ServerQuery),
	Completed {
		response: LlmProxyResponse,
	},
	Error {
		message: String,
	},
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

/// POST /proxy/anthropic/complete - Synchronous Anthropic completion.
#[axum::debug_handler]
pub async fn proxy_anthropic_complete(
	State(state): State<AppState>,
	Json(request): Json<LlmRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let service = state.llm_service.as_ref().ok_or_else(|| {
		tracing::error!("proxy_anthropic_complete: LLM service not configured");
		ServerError::ServiceUnavailable("LLM service is not configured on the server".into())
	})?;

	if !service.has_anthropic() {
		tracing::error!("proxy_anthropic_complete: Anthropic provider not configured");
		return Err(ServerError::ServiceUnavailable(
			"Anthropic provider is not configured on the server".into(),
		));
	}

	tracing::debug!(
			model = %request.model,
			message_count = request.messages.len(),
			tool_count = request.tools.len(),
			"proxy_anthropic_complete: sending request"
	);

	let response = service
		.complete_anthropic(request)
		.await
		.map_err(map_llm_error)?;

	tracing::info!(
			finish_reason = ?response.finish_reason,
			tool_call_count = response.tool_calls.len(),
			"proxy_anthropic_complete: returning response"
	);

	Ok((StatusCode::OK, Json(LlmProxyResponse::from(response))))
}

/// POST /proxy/anthropic/stream - Streaming Anthropic completion via SSE.
#[axum::debug_handler]
pub async fn proxy_anthropic_stream(
	State(state): State<AppState>,
	Json(request): Json<LlmRequest>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ServerError> {
	let service = state.llm_service.as_ref().ok_or_else(|| {
		tracing::error!("proxy_anthropic_stream: LLM service not configured");
		ServerError::ServiceUnavailable("LLM service is not configured on the server".into())
	})?;

	if !service.has_anthropic() {
		tracing::error!("proxy_anthropic_stream: Anthropic provider not configured");
		return Err(ServerError::ServiceUnavailable(
			"Anthropic provider is not configured on the server".into(),
		));
	}

	tracing::debug!(
			model = %request.model,
			message_count = request.messages.len(),
			tool_count = request.tools.len(),
			"proxy_anthropic_stream: starting stream"
	);

	let stream = service
		.complete_streaming_anthropic(request)
		.await
		.map_err(map_llm_error)?;
	Ok(create_sse_response(stream))
}

/// POST /proxy/openai/complete - Synchronous OpenAI completion.
#[axum::debug_handler]
pub async fn proxy_openai_complete(
	State(state): State<AppState>,
	Json(request): Json<LlmRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let service = state.llm_service.as_ref().ok_or_else(|| {
		tracing::error!("proxy_openai_complete: LLM service not configured");
		ServerError::ServiceUnavailable("LLM service is not configured on the server".into())
	})?;

	if !service.has_openai() {
		tracing::error!("proxy_openai_complete: OpenAI provider not configured");
		return Err(ServerError::ServiceUnavailable(
			"OpenAI provider is not configured on the server".into(),
		));
	}

	tracing::debug!(
			model = %request.model,
			message_count = request.messages.len(),
			tool_count = request.tools.len(),
			"proxy_openai_complete: sending request"
	);

	let response = service
		.complete_openai(request)
		.await
		.map_err(map_llm_error)?;

	tracing::info!(
			finish_reason = ?response.finish_reason,
			tool_call_count = response.tool_calls.len(),
			"proxy_openai_complete: returning response"
	);

	Ok((StatusCode::OK, Json(LlmProxyResponse::from(response))))
}

/// POST /proxy/openai/stream - Streaming OpenAI completion via SSE.
#[axum::debug_handler]
pub async fn proxy_openai_stream(
	State(state): State<AppState>,
	Json(request): Json<LlmRequest>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ServerError> {
	let service = state.llm_service.as_ref().ok_or_else(|| {
		tracing::error!("proxy_openai_stream: LLM service not configured");
		ServerError::ServiceUnavailable("LLM service is not configured on the server".into())
	})?;

	if !service.has_openai() {
		tracing::error!("proxy_openai_stream: OpenAI provider not configured");
		return Err(ServerError::ServiceUnavailable(
			"OpenAI provider is not configured on the server".into(),
		));
	}

	tracing::debug!(
			model = %request.model,
			message_count = request.messages.len(),
			tool_count = request.tools.len(),
			"proxy_openai_stream: starting stream"
	);

	let stream = service
		.complete_streaming_openai(request)
		.await
		.map_err(map_llm_error)?;
	Ok(create_sse_response(stream))
}

/// POST /proxy/vertex/complete - Synchronous Vertex AI completion.
#[axum::debug_handler]
pub async fn proxy_vertex_complete(
	State(state): State<AppState>,
	Json(request): Json<LlmRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let service = state.llm_service.as_ref().ok_or_else(|| {
		tracing::error!("proxy_vertex_complete: LLM service not configured");
		ServerError::ServiceUnavailable("LLM service is not configured on the server".into())
	})?;

	if !service.has_vertex() {
		tracing::error!("proxy_vertex_complete: Vertex provider not configured");
		return Err(ServerError::ServiceUnavailable(
			"Vertex provider is not configured on the server".into(),
		));
	}

	tracing::debug!(
			model = %request.model,
			message_count = request.messages.len(),
			tool_count = request.tools.len(),
			"proxy_vertex_complete: sending request"
	);

	let response = service
		.complete_vertex(request)
		.await
		.map_err(map_llm_error)?;

	tracing::info!(
			finish_reason = ?response.finish_reason,
			tool_call_count = response.tool_calls.len(),
			"proxy_vertex_complete: returning response"
	);

	Ok((StatusCode::OK, Json(LlmProxyResponse::from(response))))
}

/// POST /proxy/vertex/stream - Streaming Vertex AI completion via SSE.
#[axum::debug_handler]
pub async fn proxy_vertex_stream(
	State(state): State<AppState>,
	Json(request): Json<LlmRequest>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ServerError> {
	let service = state.llm_service.as_ref().ok_or_else(|| {
		tracing::error!("proxy_vertex_stream: LLM service not configured");
		ServerError::ServiceUnavailable("LLM service is not configured on the server".into())
	})?;

	if !service.has_vertex() {
		tracing::error!("proxy_vertex_stream: Vertex provider not configured");
		return Err(ServerError::ServiceUnavailable(
			"Vertex provider is not configured on the server".into(),
		));
	}

	tracing::debug!(
		model = %request.model,
		message_count = request.messages.len(),
		tool_count = request.tools.len(),
		"proxy_vertex_stream: starting stream"
	);

	let stream = service
		.complete_streaming_vertex(request)
		.await
		.map_err(map_llm_error)?;
	Ok(create_sse_response(stream))
}

/// POST /proxy/zai/complete - Synchronous Z.ai completion.
#[axum::debug_handler]
pub async fn proxy_zai_complete(
	State(state): State<AppState>,
	Json(request): Json<LlmRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let service = state.llm_service.as_ref().ok_or_else(|| {
		tracing::error!("proxy_zai_complete: LLM service not configured");
		ServerError::ServiceUnavailable("LLM service is not configured on the server".into())
	})?;

	if !service.has_zai() {
		tracing::error!("proxy_zai_complete: Z.ai provider not configured");
		return Err(ServerError::ServiceUnavailable(
			"Z.ai provider is not configured on the server".into(),
		));
	}

	tracing::debug!(
		model = %request.model,
		message_count = request.messages.len(),
		tool_count = request.tools.len(),
		"proxy_zai_complete: sending request"
	);

	let response = service.complete_zai(request).await.map_err(map_llm_error)?;

	tracing::info!(
		finish_reason = ?response.finish_reason,
		tool_call_count = response.tool_calls.len(),
		"proxy_zai_complete: returning response"
	);

	Ok((StatusCode::OK, Json(LlmProxyResponse::from(response))))
}

/// POST /proxy/zai/stream - Streaming Z.ai completion via SSE.
#[axum::debug_handler]
pub async fn proxy_zai_stream(
	State(state): State<AppState>,
	Json(request): Json<LlmRequest>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ServerError> {
	let service = state.llm_service.as_ref().ok_or_else(|| {
		tracing::error!("proxy_zai_stream: LLM service not configured");
		ServerError::ServiceUnavailable("LLM service is not configured on the server".into())
	})?;

	if !service.has_zai() {
		tracing::error!("proxy_zai_stream: Z.ai provider not configured");
		return Err(ServerError::ServiceUnavailable(
			"Z.ai provider is not configured on the server".into(),
		));
	}

	tracing::debug!(
		model = %request.model,
		message_count = request.messages.len(),
		tool_count = request.tools.len(),
		"proxy_zai_stream: starting stream"
	);

	let stream = service
		.complete_streaming_zai(request)
		.await
		.map_err(map_llm_error)?;
	Ok(create_sse_response(stream))
}

/// Creates an SSE response from an LlmStream.
///
/// # Server Query Integration (Phase 2)
///
/// This function will be extended to check for pending server queries via the
/// ServerQueryManager and interleave them with LLM events. Server queries will
/// be:
///
/// 1. Checked after each LLM event using `query_manager.list_pending(session_id)`
/// 2. Sent as `LlmStreamEvent::ServerQuery` over SSE with `event: llm`
/// 3. Awaited for client responses via the `/api/sessions/{session_id}/query-response` endpoint
///
/// Current infrastructure supports this:
/// - `LlmStreamEvent::ServerQuery` variant defined
/// - Serialization/deserialization tested
/// - Parser in `ProxyLlmStream` handles conversion
fn create_sse_response(
	stream: LlmStream,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
	let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

	tokio::spawn(async move {
		let mut stream = stream;
		while let Some(event) = stream.next().await {
			let sse_event = match event {
				LlmEvent::TextDelta { content } => {
					let stream_event = LlmStreamEvent::TextDelta { content };
					match serde_json::to_string(&stream_event) {
						Ok(json) => Event::default().event("llm").data(json),
						Err(e) => {
							tracing::error!(error = %e, "failed to serialize text delta");
							continue;
						}
					}
				}
				LlmEvent::ToolCallDelta {
					call_id,
					tool_name,
					arguments_fragment,
				} => {
					let stream_event = LlmStreamEvent::ToolCallDelta {
						call_id,
						tool_name,
						arguments_fragment,
					};
					match serde_json::to_string(&stream_event) {
						Ok(json) => Event::default().event("llm").data(json),
						Err(e) => {
							tracing::error!(error = %e, "failed to serialize tool call delta");
							continue;
						}
					}
				}
				LlmEvent::Completed(response) => {
					tracing::info!(
							finish_reason = ?response.finish_reason,
							tool_call_count = response.tool_calls.len(),
							"stream completed"
					);
					let stream_event = LlmStreamEvent::Completed {
						response: LlmProxyResponse::from(response),
					};
					match serde_json::to_string(&stream_event) {
						Ok(json) => Event::default().event("llm").data(json),
						Err(e) => {
							tracing::error!(error = %e, "failed to serialize completed event");
							continue;
						}
					}
				}
				LlmEvent::Error(err) => {
					tracing::warn!(error = %err, "stream error");
					let stream_event = LlmStreamEvent::Error {
						message: err.to_string(),
					};
					match serde_json::to_string(&stream_event) {
						Ok(json) => Event::default().event("llm").data(json),
						Err(e) => {
							tracing::error!(error = %e, "failed to serialize error event");
							continue;
						}
					}
				}
			};

			if tx.send(Ok(sse_event)).await.is_err() {
				tracing::debug!("client disconnected");
				break;
			}
		}
	});

	Sse::new(ReceiverStream::new(rx))
}

/// Map LlmError to ServerError for HTTP response conversion.
pub fn map_llm_error(err: LlmError) -> ServerError {
	match err {
		LlmError::Http(msg) => {
			tracing::error!(error = %msg, "LLM HTTP error");
			ServerError::UpstreamError(format!("LLM HTTP error: {msg}"))
		}
		LlmError::Api(msg) => {
			tracing::warn!(error = %msg, "LLM API error");
			ServerError::UpstreamError(format!("LLM API error: {msg}"))
		}
		LlmError::Timeout => {
			tracing::warn!("LLM request timed out");
			ServerError::UpstreamTimeout("LLM request timed out".into())
		}
		LlmError::InvalidResponse(msg) => {
			tracing::error!(error = %msg, "Invalid LLM response");
			ServerError::UpstreamError(format!("Invalid LLM response: {msg}"))
		}
		LlmError::RateLimited { retry_after_secs } => {
			tracing::warn!(retry_after = ?retry_after_secs, "LLM rate limited");
			let msg = match retry_after_secs {
				Some(secs) => format!("LLM rate limited; retry after {secs} seconds"),
				None => "LLM rate limited; try again later".to_string(),
			};
			ServerError::ServiceUnavailable(msg)
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
			fn server_query_stream_event_serialization_roundtrip(query_id in "Q-[a-f0-9]{32}") {
					use loom_common_core::server_query::ServerQueryKind;

					let query = loom_common_core::server_query::ServerQuery {
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
