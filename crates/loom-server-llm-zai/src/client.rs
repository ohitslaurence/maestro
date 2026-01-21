// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Z.ai (智谱AI/ZhipuAI) API client implementation.

use crate::stream::ZaiStream;
use crate::types::{ZaiConfig, ZaiError, ZaiRequest, ZaiResponse};
use async_trait::async_trait;
use futures::Stream;
use loom_common_core::{LlmClient, LlmError, LlmEvent, LlmRequest, LlmResponse, LlmStream};
use loom_common_http::{retry, RetryConfig, RetryableError};
use reqwest::Client;
use std::pin::Pin;
use std::time::Duration;
use tracing::{debug, error, info, instrument, trace};

/// Error wrapper for retry compatibility.
#[derive(Debug)]
struct ZaiRequestError(LlmError);

impl RetryableError for ZaiRequestError {
	fn is_retryable(&self) -> bool {
		matches!(
			&self.0,
			LlmError::Http(_) | LlmError::Timeout | LlmError::RateLimited { .. }
		)
	}
}

/// Z.ai API client.
///
/// Implements the LlmClient trait for interacting with Z.ai's Chat Completions API.
/// Z.ai uses an OpenAI-compatible API format.
pub struct ZaiClient {
	config: ZaiConfig,
	http_client: Client,
	retry_config: RetryConfig,
}

impl ZaiClient {
	pub fn new(config: ZaiConfig) -> Result<Self, LlmError> {
		let http_client = loom_common_http::builder()
			.timeout(Duration::from_secs(300))
			.build()
			.map_err(|e| LlmError::Http(e.to_string()))?;

		let retry_config = RetryConfig {
			max_attempts: 3,
			base_delay: Duration::from_millis(500),
			max_delay: Duration::from_secs(30),
			backoff_factor: 2.0,
			jitter: true,
			retryable_statuses: vec![
				reqwest::StatusCode::TOO_MANY_REQUESTS,
				reqwest::StatusCode::REQUEST_TIMEOUT,
				reqwest::StatusCode::INTERNAL_SERVER_ERROR,
				reqwest::StatusCode::BAD_GATEWAY,
				reqwest::StatusCode::SERVICE_UNAVAILABLE,
				reqwest::StatusCode::GATEWAY_TIMEOUT,
			],
		};

		info!(
				model = %config.model,
				base_url = %config.base_url,
				"Initialized Z.ai client"
		);

		Ok(Self {
			config,
			http_client,
			retry_config,
		})
	}

	pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
		self.retry_config = retry_config;
		self
	}

	fn build_request(&self, request: &LlmRequest, stream: bool) -> reqwest::RequestBuilder {
		let zai_request = ZaiRequest::from_llm_request(request, stream);
		let url = format!("{}/chat/completions", self.config.base_url);

		let builder = self
			.http_client
			.post(&url)
			.header("Content-Type", "application/json")
			.header("Authorization", format!("Bearer {}", self.config.api_key));

		trace!(
				url = %url,
				model = %request.model,
				stream = stream,
				message_count = request.messages.len(),
				tool_count = request.tools.len(),
				"Building Z.ai request"
		);

		builder.json(&zai_request)
	}

	async fn handle_error_response(&self, response: reqwest::Response) -> LlmError {
		let status = response.status();
		let status_code = status.as_u16();

		debug!(status = %status, "Received error response from Z.ai");

		if status_code == 401 {
			return LlmError::Api("Authentication failed".to_string());
		}

		if status_code == 429 {
			let retry_after = response
				.headers()
				.get("retry-after")
				.and_then(|v| v.to_str().ok())
				.and_then(|v| v.parse().ok());

			return LlmError::RateLimited {
				retry_after_secs: retry_after,
			};
		}

		match response.json::<ZaiError>().await {
			Ok(error) => {
				error!(
						error_type = ?error.error.error_type,
						code = ?error.error.code,
						message = %error.error.message,
						"Z.ai API error"
				);
				LlmError::Api(error.error.message)
			}
			Err(e) => {
				error!(
						status = %status,
						parse_error = %e,
						"Failed to parse Z.ai error response"
				);
				LlmError::Api(format!("HTTP {status}"))
			}
		}
	}
}

#[async_trait]
impl LlmClient for ZaiClient {
	#[instrument(skip(self, request), fields(model = %request.model))]
	async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
		debug!(
			message_count = request.messages.len(),
			tool_count = request.tools.len(),
			"Starting completion request"
		);

		let result = retry(&self.retry_config, || async {
			let req = self.build_request(&request, false);

			let response = req.send().await.map_err(|e| {
				if e.is_timeout() {
					ZaiRequestError(LlmError::Timeout)
				} else {
					ZaiRequestError(LlmError::Http(e.to_string()))
				}
			})?;

			if !response.status().is_success() {
				let error = self.handle_error_response(response).await;
				return Err(ZaiRequestError(error));
			}

			let zai_response: ZaiResponse = response
				.json()
				.await
				.map_err(|e| ZaiRequestError(LlmError::InvalidResponse(e.to_string())))?;

			trace!(
					response_id = %zai_response.id,
					model = %zai_response.model,
					"Received Z.ai response"
			);

			Ok(LlmResponse::from(zai_response))
		})
		.await;

		match result {
			Ok(response) => {
				info!(
						content_len = response.message.content.len(),
						tool_calls = response.tool_calls.len(),
						input_tokens = response.usage.as_ref().map(|u| u.input_tokens).unwrap_or(0),
						output_tokens = response.usage.as_ref().map(|u| u.output_tokens).unwrap_or(0),
						finish_reason = ?response.finish_reason,
						"Completion request successful"
				);
				Ok(response)
			}
			Err(e) => {
				error!(error = ?e, "Request failed");
				Err(e.0)
			}
		}
	}

	#[instrument(skip(self, request), fields(model = %request.model))]
	async fn complete_streaming(&self, request: LlmRequest) -> Result<LlmStream, LlmError> {
		debug!(
			message_count = request.messages.len(),
			tool_count = request.tools.len(),
			"Starting streaming completion request"
		);

		let req = self.build_request(&request, true);
		let response = req.send().await.map_err(|e| {
			if e.is_timeout() {
				LlmError::Timeout
			} else {
				LlmError::Http(e.to_string())
			}
		})?;

		if !response.status().is_success() {
			return Err(self.handle_error_response(response).await);
		}

		info!("Streaming response initiated");

		let byte_stream = response.bytes_stream();
		let event_stream = ZaiStream::new(byte_stream);
		let boxed: Pin<Box<dyn Stream<Item = LlmEvent> + Send>> = Box::pin(event_stream);

		Ok(LlmStream::new(boxed))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Tests that the client correctly initializes with default configuration.
	/// This is important to ensure the client can be created without panics
	/// and has sensible defaults for production use.
	#[test]
	fn test_client_creation() {
		let config = ZaiConfig::new("test-api-key")
			.with_model("glm-4.7")
			.with_base_url("https://api.z.ai/api/paas/v4");

		let client = ZaiClient::new(config);
		assert!(client.is_ok());
	}

	/// Tests that custom retry configuration can be applied to the client.
	/// This is important because different use cases may require different
	/// retry strategies (e.g., more retries for batch processing).
	#[test]
	fn test_client_with_custom_retry_config() {
		let config = ZaiConfig::new("test-api-key");
		let retry_config = RetryConfig {
			max_attempts: 5,
			base_delay: Duration::from_secs(1),
			max_delay: Duration::from_secs(60),
			backoff_factor: 2.0,
			jitter: true,
			retryable_statuses: vec![],
		};

		let client = ZaiClient::new(config)
			.unwrap()
			.with_retry_config(retry_config);

		assert_eq!(client.retry_config.max_attempts, 5);
	}
}
