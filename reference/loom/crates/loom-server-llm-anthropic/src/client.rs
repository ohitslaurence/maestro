// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Anthropic client implementation.

use async_trait::async_trait;
use loom_cli_credentials::{CredentialStore, MemoryCredentialStore};
use loom_common_core::{LlmClient, LlmError, LlmRequest, LlmResponse, LlmStream};
use loom_common_http::{retry, RetryConfig, RetryableError};
use reqwest::Client;
use tracing::{debug, error, info, instrument, trace};

use crate::stream::parse_sse_stream;
use crate::types::{AnthropicConfig, AnthropicError, AnthropicRequest, AnthropicResponse};

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Classification of client errors for failover behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientErrorKind {
	/// Transient error, retry on same account (via loom_common_http)
	Transient,
	/// Quota exhausted, failover to next account
	QuotaExceeded,
	/// Permanent error (bad credentials), disable account
	Permanent,
}

#[derive(Debug)]
pub struct ClientError {
	message: String,
	retryable: bool,
	kind: ClientErrorKind,
}

impl ClientError {
	/// Returns the error classification kind
	pub fn kind(&self) -> ClientErrorKind {
		self.kind
	}
}

/// Detects if an error message indicates 5-hour quota exhaustion
/// Check if an error message indicates quota exhaustion (5-hour rolling limit).
///
/// This is used for pool failover decisions and is shared with the pool module.
pub fn is_quota_message(msg: &str) -> bool {
	let lower = msg.to_ascii_lowercase();
	lower.contains("5-hour")
		|| lower.contains("5 hour")
		|| lower.contains("rolling window")
		|| lower.contains("usage limit for your plan")
		|| lower.contains("subscription usage limit")
}

/// Check if an error message indicates a permanent auth failure.
///
/// This is used for pool failover decisions and is shared with the pool module.
pub fn is_permanent_auth_message(msg: &str) -> bool {
	let lower = msg.to_ascii_lowercase();
	lower.contains("401")
		|| lower.contains("403")
		|| lower.contains("unauthorized")
		|| lower.contains("forbidden")
		|| lower.contains("invalid api key")
		|| lower.contains("invalid authentication")
		|| lower.contains("authentication failed")
		|| lower.contains("invalid token")
		|| lower.contains("expired token")
}

/// Classifies an HTTP error based on status code and message
fn classify_error(status: u16, message: &str) -> ClientErrorKind {
	if status == 401 || status == 403 {
		return ClientErrorKind::Permanent;
	}

	if status == 429 && is_quota_message(message) {
		return ClientErrorKind::QuotaExceeded;
	}

	if matches!(status, 408 | 429 | 500 | 502 | 503 | 504) {
		return ClientErrorKind::Transient;
	}

	ClientErrorKind::Permanent
}

impl std::fmt::Display for ClientError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.message)
	}
}

impl std::error::Error for ClientError {}

impl RetryableError for ClientError {
	fn is_retryable(&self) -> bool {
		self.retryable
	}
}

impl From<ClientError> for LlmError {
	fn from(err: ClientError) -> Self {
		LlmError::Http(err.message)
	}
}

/// Client for interacting with Anthropic's Claude API.
#[derive(Debug)]
pub struct AnthropicClient<S: CredentialStore = MemoryCredentialStore> {
	config: AnthropicConfig<S>,
	http_client: Client,
	retry_config: RetryConfig,
}

impl<S: CredentialStore> Clone for AnthropicClient<S> {
	fn clone(&self) -> Self {
		Self {
			config: self.config.clone(),
			http_client: self.http_client.clone(),
			retry_config: self.retry_config.clone(),
		}
	}
}

impl AnthropicClient<MemoryCredentialStore> {
	/// Create a new client with default memory credential store.
	pub fn new(config: AnthropicConfig<MemoryCredentialStore>) -> Result<Self, LlmError> {
		Self::new_with_store(config)
	}
}

impl<S: CredentialStore + 'static> AnthropicClient<S> {
	/// Create a new client with a specific credential store.
	pub fn new_with_store(config: AnthropicConfig<S>) -> Result<Self, LlmError> {
		use crate::auth::ANTHROPIC_USER_AGENT;

		let http_client = loom_common_http::builder_with_user_agent(ANTHROPIC_USER_AGENT)
			.build()
			.map_err(|e| LlmError::Http(format!("Failed to create HTTP client: {e}")))?;

		Ok(Self {
			config,
			http_client,
			retry_config: RetryConfig::default(),
		})
	}

	pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
		self.retry_config = retry_config;
		self
	}

	fn messages_url(&self) -> String {
		format!("{}/v1/messages", self.config.base_url)
	}

	#[instrument(skip(self, request), fields(model = %request.model))]
	async fn send_request(
		&self,
		request: &AnthropicRequest,
	) -> Result<reqwest::Response, ClientError> {
		let url = self.messages_url();
		debug!(url = %url, "Sending request to Anthropic API");
		trace!(request = ?request, "Request payload");

		let builder = self
			.http_client
			.post(&url)
			.header("anthropic-version", ANTHROPIC_VERSION)
			.header("content-type", "application/json")
			.json(request);

		let builder = self
			.config
			.auth
			.apply_to_request(builder)
			.await
			.map_err(|e| ClientError {
				message: format!("Auth error: {e}"),
				retryable: false,
				kind: ClientErrorKind::Permanent,
			})?;

		let response = builder.send().await.map_err(|e| {
			let retryable = e.is_timeout() || e.is_connect();
			error!(error = %e, retryable = retryable, "HTTP request failed");
			ClientError {
				message: e.to_string(),
				retryable,
				kind: if retryable {
					ClientErrorKind::Transient
				} else {
					ClientErrorKind::Permanent
				},
			}
		})?;

		let status = response.status();
		debug!(status = %status, "Received response");

		if !status.is_success() {
			let error_body = response.text().await.unwrap_or_default();

			let message = if let Ok(api_error) = serde_json::from_str::<AnthropicError>(&error_body) {
				api_error.error.message
			} else {
				error_body
			};

			let kind = classify_error(status.as_u16(), &message);
			let retryable = kind == ClientErrorKind::Transient;
			error!(status = %status, body = %message, retryable = retryable, kind = ?kind, "API error response");

			return Err(ClientError {
				message,
				retryable,
				kind,
			});
		}

		Ok(response)
	}
}

#[async_trait]
impl<S: CredentialStore + 'static> LlmClient for AnthropicClient<S> {
	#[instrument(skip(self, request), fields(model = %request.model))]
	async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
		info!("Starting non-streaming completion request");

		let mut anthropic_request = AnthropicRequest::from(&request);
		anthropic_request.stream = Some(false);

		// Apply OAuth system prompt prefix for Opus/Sonnet access
		if self.config.auth.is_oauth() {
			anthropic_request = anthropic_request.with_oauth_system_prompt();
		}

		let client = self.clone();
		let anthropic_request_clone = anthropic_request.clone();
		let response = retry(&self.retry_config, || {
			let req = anthropic_request_clone.clone();
			let c = client.clone();
			async move { c.send_request(&req).await }
		})
		.await
		.map_err(LlmError::from)?;

		let response_body = response.text().await.map_err(|e| {
			error!(error = %e, "Failed to read response body");
			LlmError::Http(e.to_string())
		})?;

		trace!(body = %response_body, "Response body");

		let anthropic_response: AnthropicResponse =
			serde_json::from_str(&response_body).map_err(|e| {
				error!(error = %e, body = %response_body, "Failed to parse response");
				LlmError::InvalidResponse(format!("Failed to parse response: {e}"))
			})?;

		let llm_response = LlmResponse::try_from(anthropic_response)?;
		info!(
				finish_reason = ?llm_response.finish_reason,
				"Completion request finished"
		);

		Ok(llm_response)
	}

	#[instrument(skip(self, request), fields(model = %request.model))]
	async fn complete_streaming(&self, request: LlmRequest) -> Result<LlmStream, LlmError> {
		info!("Starting streaming completion request");

		let mut anthropic_request = AnthropicRequest::from(&request);
		anthropic_request.stream = Some(true);

		// Apply OAuth system prompt prefix for Opus/Sonnet access
		if self.config.auth.is_oauth() {
			anthropic_request = anthropic_request.with_oauth_system_prompt();
		}

		let client = self.clone();
		let anthropic_request_clone = anthropic_request.clone();
		let response = retry(&self.retry_config, || {
			let req = anthropic_request_clone.clone();
			let c = client.clone();
			async move { c.send_request(&req).await }
		})
		.await
		.map_err(LlmError::from)?;

		debug!("Stream connection established");
		let stream = parse_sse_stream(response.bytes_stream());

		Ok(LlmStream::new(Box::pin(stream)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_client_creation() {
		let config = AnthropicConfig::new("test-key");
		let client = AnthropicClient::new(config);
		assert!(client.is_ok());
	}

	#[test]
	fn test_messages_url() {
		let config = AnthropicConfig::new("test-key").with_base_url("https://custom.api.com");
		let client = AnthropicClient::new(config).unwrap();
		assert_eq!(client.messages_url(), "https://custom.api.com/v1/messages");
	}
}
