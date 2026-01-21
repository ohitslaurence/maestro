// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! LLM abstraction types for request/response handling and streaming.

use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use futures::Stream;
use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::error::LlmError;
use crate::message::{Message, ToolCall};
use crate::tool::ToolDefinition;

/// Request to send to an LLM for completion.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmRequest {
	pub model: String,
	pub messages: Vec<Message>,
	pub tools: Vec<ToolDefinition>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub max_tokens: Option<u32>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub temperature: Option<f32>,
}

impl LlmRequest {
	pub fn new(model: impl Into<String>) -> Self {
		Self {
			model: model.into(),
			messages: Vec::new(),
			tools: Vec::new(),
			max_tokens: None,
			temperature: None,
		}
	}

	pub fn with_model(mut self, model: impl Into<String>) -> Self {
		self.model = model.into();
		self
	}

	pub fn with_messages(mut self, messages: Vec<Message>) -> Self {
		self.messages = messages;
		self
	}

	pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
		self.tools = tools;
		self
	}

	pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
		self.max_tokens = Some(max_tokens);
		self
	}

	pub fn with_temperature(mut self, temperature: f32) -> Self {
		self.temperature = Some(temperature);
		self
	}
}

/// Streaming events emitted by an LLM during completion.
#[derive(Clone, Debug)]
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

/// Response from an LLM completion request.
#[derive(Clone, Debug)]
pub struct LlmResponse {
	pub message: Message,
	pub tool_calls: Vec<ToolCall>,
	pub usage: Option<Usage>,
	pub finish_reason: Option<String>,
}

/// Token usage statistics from an LLM request.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Usage {
	pub input_tokens: u32,
	pub output_tokens: u32,
}

pin_project! {
		/// A stream of LLM events during completion.
		///
		/// Wraps an async stream of [`LlmEvent`] items, providing both
		/// direct async iteration via [`next`] and [`Stream`] trait implementation.
		pub struct LlmStream {
				#[pin]
				inner: Pin<Box<dyn Stream<Item = LlmEvent> + Send>>,
		}
}

impl LlmStream {
	/// Creates a new LLM stream from a boxed stream.
	pub fn new(inner: Pin<Box<dyn Stream<Item = LlmEvent> + Send>>) -> Self {
		Self { inner }
	}

	/// Returns the next event from the stream, or `None` if the stream is
	/// exhausted.
	#[instrument(skip(self), level = "trace")]
	pub async fn next(&mut self) -> Option<LlmEvent> {
		use futures::StreamExt;
		self.inner.next().await
	}
}

impl Stream for LlmStream {
	type Item = LlmEvent;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		self.project().inner.poll_next(cx)
	}
}

/// Trait for LLM client implementations.
///
/// Provides both blocking and streaming completion methods for interacting
/// with language model providers.
#[async_trait]
pub trait LlmClient: Send + Sync {
	/// Sends a completion request and waits for the full response.
	async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError>;

	/// Sends a completion request and returns a stream of events.
	async fn complete_streaming(&self, request: LlmRequest) -> Result<LlmStream, LlmError>;
}

#[cfg(test)]
mod tests {
	use super::*;

	mod llm_request {
		use super::*;
		use proptest::prelude::*;

		proptest! {
				/// Verifies LlmRequest can be serialized to JSON and deserialized back,
				/// ensuring round-trip consistency for API communication.
				#[test]
				fn serialization_roundtrip_preserves_data(
						model in "[a-z]{1,20}",
						max_tokens in proptest::option::of(1u32..10000),
						temperature in proptest::option::of(0.0f32..2.0),
				) {
						let request = LlmRequest {
								model,
								messages: vec![],
								tools: vec![],
								max_tokens,
								temperature,
						};

						let json = serde_json::to_string(&request).expect("serialization should succeed");
						let deserialized: LlmRequest = serde_json::from_str(&json).expect("deserialization should succeed");

						prop_assert_eq!(request.model, deserialized.model);
						prop_assert_eq!(request.max_tokens, deserialized.max_tokens);
						prop_assert!((request.temperature.unwrap_or(0.0) - deserialized.temperature.unwrap_or(0.0)).abs() < f32::EPSILON);
				}
		}
	}

	mod usage {
		use super::*;
		use proptest::prelude::*;

		proptest! {
				/// Validates that Usage struct serialization is consistent,
				/// important for accurate token tracking and billing.
				#[test]
				fn serialization_roundtrip_preserves_tokens(
						input_tokens in 0u32..1_000_000,
						output_tokens in 0u32..1_000_000,
				) {
						let usage = Usage { input_tokens, output_tokens };

						let json = serde_json::to_string(&usage).expect("serialization should succeed");
						let deserialized: Usage = serde_json::from_str(&json).expect("deserialization should succeed");

						prop_assert_eq!(usage.input_tokens, deserialized.input_tokens);
						prop_assert_eq!(usage.output_tokens, deserialized.output_tokens);
				}
		}
	}
}
