// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Oracle tool that queries OpenAI for additional reasoning/advice via the Loom
//! server proxy.

use async_trait::async_trait;
use loom_common_core::{LlmRequest, Message, ToolContext, ToolError};
use loom_common_http::{retry, RetryConfig, RetryableError};
use reqwest::Client;
use serde::Deserialize;
use std::fmt;
use std::time::Duration;

use crate::Tool;

const DEFAULT_MODEL: &str = "gpt-4o";
const DEFAULT_MAX_TOKENS: u32 = 512;
const DEFAULT_TEMPERATURE: f32 = 0.2;
const MIN_MAX_TOKENS: u32 = 16;
const MAX_MAX_TOKENS: u32 = 4096;
const MIN_TEMPERATURE: f32 = 0.0;
const MAX_TEMPERATURE: f32 = 2.0;

const DEFAULT_SYSTEM_PROMPT: &str = "You are the 'oracle' sub-agent for Loom. You are being \
                                     called by another AI model (Claude) to provide additional \
                                     reasoning or advice. Answer concisely and focus on technical \
                                     correctness and clarity.";

#[derive(Debug, Deserialize)]
struct OracleArgs {
	query: String,
	model: Option<String>,
	max_tokens: Option<u32>,
	temperature: Option<f32>,
	system_prompt: Option<String>,
}

#[derive(Debug)]
enum OracleError {
	Request(reqwest::Error),
	Http { status: u16, body: String },
}

impl fmt::Display for OracleError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			OracleError::Request(e) => write!(f, "request error: {e}"),
			OracleError::Http { status, body } => write!(f, "HTTP {status}: {body}"),
		}
	}
}

impl std::error::Error for OracleError {}

impl RetryableError for OracleError {
	fn is_retryable(&self) -> bool {
		match self {
			OracleError::Request(e) => e.is_retryable(),
			OracleError::Http { status, .. } => matches!(*status, 429 | 408 | 502 | 503 | 504),
		}
	}
}

pub struct OracleTool {
	client: Client,
	base_url: String,
}

impl OracleTool {
	pub fn new(base_url: impl Into<String>) -> Self {
		Self {
			client: loom_common_http::new_client(),
			base_url: base_url.into(),
		}
	}
}

impl Default for OracleTool {
	fn default() -> Self {
		let base_url =
			std::env::var("LOOM_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
		Self::new(base_url)
	}
}

/// Builds an LlmRequest for the oracle query.
///
/// Clamps max_tokens to 16-4096 and temperature to 0.0-2.0.
pub fn build_llm_request(
	query: &str,
	model: Option<&str>,
	max_tokens: Option<u32>,
	temperature: Option<f32>,
	system_prompt: Option<&str>,
) -> LlmRequest {
	let model = model
		.map(|s| s.to_string())
		.or_else(|| std::env::var("LOOM_SERVER_ORACLE_MODEL").ok())
		.unwrap_or_else(|| DEFAULT_MODEL.to_string());

	let max_tokens = max_tokens
		.unwrap_or(DEFAULT_MAX_TOKENS)
		.clamp(MIN_MAX_TOKENS, MAX_MAX_TOKENS);

	let temperature = temperature
		.unwrap_or(DEFAULT_TEMPERATURE)
		.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE);

	let system_content = system_prompt.unwrap_or(DEFAULT_SYSTEM_PROMPT);

	let messages = vec![Message::system(system_content), Message::user(query)];

	LlmRequest::new(model)
		.with_messages(messages)
		.with_max_tokens(max_tokens)
		.with_temperature(temperature)
}

#[async_trait]
impl Tool for OracleTool {
	fn name(&self) -> &str {
		"oracle"
	}

	fn description(&self) -> &str {
		"Query OpenAI for additional reasoning or advice via the Loom server proxy. Use this tool when \
		 you need a second opinion, help with complex reasoning, or specialized knowledge."
	}

	fn input_schema(&self) -> serde_json::Value {
		serde_json::json!({
				"type": "object",
				"properties": {
						"query": {
								"type": "string",
								"description": "The question or task for OpenAI to reason about."
						},
						"model": {
								"type": "string",
								"description": "Model override. Defaults to LOOM_ORACLE_MODEL env or 'gpt-4o'."
						},
						"max_tokens": {
								"type": "integer",
								"minimum": 16,
								"maximum": 4096,
								"description": "Maximum tokens in the response (default: 512)."
						},
						"temperature": {
								"type": "number",
								"minimum": 0.0,
								"maximum": 2.0,
								"description": "Sampling temperature (default: 0.2)."
						},
						"system_prompt": {
								"type": "string",
								"description": "Extra guidance for the oracle to customize its behavior."
						}
				},
				"required": ["query"]
		})
	}

	async fn invoke(
		&self,
		args: serde_json::Value,
		_ctx: &ToolContext,
	) -> Result<serde_json::Value, ToolError> {
		let args: OracleArgs =
			serde_json::from_value(args).map_err(|e| ToolError::Serialization(e.to_string()))?;

		let query = args.query.trim().to_string();
		if query.is_empty() {
			return Err(ToolError::InvalidArguments(
				"query must not be empty".to_string(),
			));
		}

		let request = build_llm_request(
			&query,
			args.model.as_deref(),
			args.max_tokens,
			args.temperature,
			args.system_prompt.as_deref(),
		);

		tracing::debug!(
				query = %query,
				model = %request.model,
				max_tokens = ?request.max_tokens,
				temperature = ?request.temperature,
				"oracle: sending request to server proxy"
		);

		let url = format!(
			"{}/proxy/openai/complete",
			self.base_url.trim_end_matches('/')
		);

		let retry_config = RetryConfig {
			max_attempts: 3,
			base_delay: Duration::from_millis(500),
			max_delay: Duration::from_secs(10),
			backoff_factor: 2.0,
			jitter: true,
			..Default::default()
		};

		let client = &self.client;
		let request_json =
			serde_json::to_value(&request).map_err(|e| ToolError::Serialization(e.to_string()))?;

		let result = retry(&retry_config, || {
			let url = url.clone();
			let request_json = request_json.clone();
			async move {
				let response = client
					.post(&url)
					.json(&request_json)
					.timeout(Duration::from_secs(60))
					.send()
					.await
					.map_err(OracleError::Request)?;

				let status = response.status();
				let body = response.text().await.map_err(OracleError::Request)?;

				if !status.is_success() {
					return Err(OracleError::Http {
						status: status.as_u16(),
						body,
					});
				}

				serde_json::from_str::<serde_json::Value>(&body)
					.or_else(|_| Ok(serde_json::json!({ "response": body })))
			}
		})
		.await;

		match result {
			Ok(value) => {
				tracing::debug!(
						query = %query,
						"oracle: received response"
				);
				Ok(value)
			}
			Err(OracleError::Request(e)) => {
				if e.is_timeout() {
					tracing::warn!(error = %e, "oracle: server timeout");
					Err(ToolError::Timeout)
				} else {
					tracing::error!(error = %e, "oracle: network error");
					Err(ToolError::Io(e.to_string()))
				}
			}
			Err(OracleError::Http { status, body }) => {
				tracing::warn!(
						status = status,
						body = %body,
						"oracle: server returned non-success"
				);
				Err(ToolError::Internal(format!(
					"oracle proxy error: HTTP {status}"
				)))
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;
	use std::path::PathBuf;

	#[tokio::test]
	async fn test_empty_query_returns_error() {
		let tool = OracleTool::new("http://localhost:8080");
		let ctx = ToolContext {
			workspace_root: PathBuf::from("/tmp"),
		};

		let result = tool.invoke(serde_json::json!({"query": ""}), &ctx).await;
		assert!(matches!(result, Err(ToolError::InvalidArguments(_))));

		let result = tool.invoke(serde_json::json!({"query": "   "}), &ctx).await;
		assert!(matches!(result, Err(ToolError::InvalidArguments(_))));

		let result = tool
			.invoke(serde_json::json!({"query": "\t\n  "}), &ctx)
			.await;
		assert!(matches!(result, Err(ToolError::InvalidArguments(_))));
	}

	proptest! {
			/// Purpose: Verifies that build_llm_request always produces exactly 2 messages
			/// (system + user), and that max_tokens and temperature are properly clamped
			/// to their valid ranges. This is critical because:
			/// 1. The oracle expects a consistent message structure for proper context
			/// 2. Out-of-range values could cause API errors or unexpected behavior
			/// 3. Clamping ensures graceful handling of edge cases without failures
			#[test]
			fn test_build_llm_request_properties(
					query in "[a-zA-Z0-9 ]{1,100}",
					model in proptest::option::of("[a-z0-9-]{1,20}"),
					max_tokens in proptest::option::of(0u32..10000),
					temperature in proptest::option::of(-1.0f32..5.0),
					system_prompt in proptest::option::of("[a-zA-Z0-9 ]{1,100}"),
			) {
					let request = build_llm_request(
							&query,
							model.as_deref(),
							max_tokens,
							temperature,
							system_prompt.as_deref(),
					);

					// Messages always has exactly 2 elements: system + user
					prop_assert_eq!(
							request.messages.len(),
							2,
							"Expected exactly 2 messages (system + user), got {}",
							request.messages.len()
					);

					// First message is system, second is user
					prop_assert_eq!(&request.messages[0].role, &loom_common_core::Role::System);
					prop_assert_eq!(&request.messages[1].role, &loom_common_core::Role::User);

					// User message contains the query
					prop_assert_eq!(&request.messages[1].content, &query);

					// max_tokens is clamped to 16-4096
					let actual_max_tokens = request.max_tokens.unwrap();
					prop_assert!(
							(MIN_MAX_TOKENS..=MAX_MAX_TOKENS).contains(&actual_max_tokens),
							"max_tokens {} not in range [{}, {}]",
							actual_max_tokens,
							MIN_MAX_TOKENS,
							MAX_MAX_TOKENS
					);

					// temperature is clamped to 0.0-2.0
					let actual_temperature = request.temperature.unwrap();
					prop_assert!(
							(MIN_TEMPERATURE..=MAX_TEMPERATURE).contains(&actual_temperature),
							"temperature {} not in range [{}, {}]",
							actual_temperature,
							MIN_TEMPERATURE,
							MAX_TEMPERATURE
					);
			}
	}
}
