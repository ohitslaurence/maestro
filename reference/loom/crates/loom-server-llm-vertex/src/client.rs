// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Vertex AI client implementation.

use async_trait::async_trait;
use gcp_auth::TokenProvider;
use loom_common_core::{LlmClient, LlmError, LlmRequest, LlmResponse, LlmStream};
use loom_common_http::{retry, RetryConfig, RetryableError};
use reqwest::Client;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, instrument, trace};

use crate::stream::parse_vertex_stream;
use crate::types::{VertexConfig, VertexError, VertexRequest, VertexResponse};

/// Cached access token with expiry tracking.
struct CachedToken {
	token: String,
	expires_at: std::time::Instant,
}

/// Error type for client operations with retry support.
#[derive(Debug)]
pub struct ClientError {
	message: String,
	retryable: bool,
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

/// Client for interacting with Google Vertex AI (Gemini) API.
///
/// Uses Application Default Credentials (ADC) for authentication.
#[derive(Clone)]
pub struct VertexClient {
	config: VertexConfig,
	http_client: Client,
	retry_config: RetryConfig,
	auth_provider: Arc<RwLock<Option<Arc<dyn TokenProvider>>>>,
	cached_token: Arc<RwLock<Option<CachedToken>>>,
}

impl std::fmt::Debug for VertexClient {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("VertexClient")
			.field("config", &self.config)
			.field("retry_config", &self.retry_config)
			.finish_non_exhaustive()
	}
}

impl VertexClient {
	/// Creates a new Vertex AI client with the given configuration.
	///
	/// Authentication is initialized lazily on first request.
	pub fn new(config: VertexConfig) -> Result<Self, LlmError> {
		let http_client = loom_common_http::builder()
			.timeout(Duration::from_secs(300))
			.build()
			.map_err(|e| LlmError::Http(format!("Failed to create HTTP client: {e}")))?;

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
				project = %config.project_id,
				location = %config.location,
				model = %config.model,
				"Initialized Vertex AI client"
		);

		Ok(Self {
			config,
			http_client,
			retry_config,
			auth_provider: Arc::new(RwLock::new(None)),
			cached_token: Arc::new(RwLock::new(None)),
		})
	}

	/// Sets a custom retry configuration.
	pub fn with_retry_config(mut self, retry_config: RetryConfig) -> Self {
		self.retry_config = retry_config;
		self
	}

	fn generate_content_url(&self, stream: bool) -> String {
		let method = if stream {
			"streamGenerateContent"
		} else {
			"generateContent"
		};
		format!(
			"{}/v1/projects/{}/locations/{}/publishers/google/models/{}:{}",
			self.config.base_url, self.config.project_id, self.config.location, self.config.model, method,
		)
	}

	/// Gets an access token, using cache if available and not expired.
	async fn get_access_token(&self) -> Result<String, ClientError> {
		{
			let cached = self.cached_token.read().await;
			if let Some(ref token) = *cached {
				if token.expires_at > std::time::Instant::now() + Duration::from_secs(60) {
					return Ok(token.token.clone());
				}
			}
		}

		let mut provider_guard = self.auth_provider.write().await;
		if provider_guard.is_none() {
			debug!("Initializing GCP authentication provider");
			let provider = gcp_auth::provider().await.map_err(|e| {
				error!(error = %e, "Failed to initialize GCP auth");
				ClientError {
					message: format!("GCP auth initialization failed: {e}"),
					retryable: false,
				}
			})?;
			*provider_guard = Some(provider);
		}

		let provider = provider_guard.as_ref().unwrap();
		let scopes = &["https://www.googleapis.com/auth/cloud-platform"];

		let token = provider.token(scopes).await.map_err(|e| {
			error!(error = %e, "Failed to get GCP access token");
			ClientError {
				message: format!("GCP token acquisition failed: {e}"),
				retryable: true,
			}
		})?;

		let token_str = token.as_str().to_string();

		{
			let mut cached = self.cached_token.write().await;
			*cached = Some(CachedToken {
				token: token_str.clone(),
				expires_at: std::time::Instant::now() + Duration::from_secs(3500),
			});
		}

		Ok(token_str)
	}

	#[instrument(skip(self, request), fields(model = %self.config.model))]
	async fn send_request(
		&self,
		request: &VertexRequest,
		stream: bool,
	) -> Result<reqwest::Response, ClientError> {
		let url = self.generate_content_url(stream);
		debug!(url = %url, stream = stream, "Sending request to Vertex AI");
		trace!(request = ?request, "Request payload");

		let token = self.get_access_token().await?;

		let response = self
			.http_client
			.post(&url)
			.bearer_auth(&token)
			.header("Content-Type", "application/json")
			.json(request)
			.send()
			.await
			.map_err(|e| {
				let retryable = e.is_timeout() || e.is_connect();
				error!(error = %e, retryable = retryable, "HTTP request failed");
				ClientError {
					message: e.to_string(),
					retryable,
				}
			})?;

		let status = response.status();
		debug!(status = %status, "Received response");

		if !status.is_success() {
			let retryable = matches!(status.as_u16(), 408 | 429 | 500 | 502 | 503 | 504);
			let error_body = response.text().await.unwrap_or_default();
			error!(status = %status, body = %error_body, retryable = retryable, "API error response");

			let message = if let Ok(api_error) = serde_json::from_str::<VertexError>(&error_body) {
				api_error.error.message
			} else {
				error_body
			};

			return Err(ClientError { message, retryable });
		}

		Ok(response)
	}
}

#[async_trait]
impl LlmClient for VertexClient {
	#[instrument(skip(self, request), fields(model = %self.config.model))]
	async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
		info!("Starting Vertex non-streaming completion request");

		let vertex_request = VertexRequest::from(&request);

		let client = self.clone();
		let vertex_request_clone = vertex_request.clone();
		let response = retry(&self.retry_config, || {
			let req = vertex_request_clone.clone();
			let c = client.clone();
			async move { c.send_request(&req, false).await }
		})
		.await
		.map_err(LlmError::from)?;

		let response_body = response.text().await.map_err(|e| {
			error!(error = %e, "Failed to read response body");
			LlmError::Http(e.to_string())
		})?;

		trace!(body = %response_body, "Response body");

		let vertex_response: VertexResponse = serde_json::from_str(&response_body).map_err(|e| {
			error!(error = %e, body = %response_body, "Failed to parse response");
			LlmError::InvalidResponse(format!("Failed to parse response: {e}"))
		})?;

		let llm_response = LlmResponse::try_from(vertex_response)?;
		info!(
				finish_reason = ?llm_response.finish_reason,
				tool_calls = llm_response.tool_calls.len(),
				"Vertex completion request finished"
		);

		Ok(llm_response)
	}

	#[instrument(skip(self, request), fields(model = %self.config.model))]
	async fn complete_streaming(&self, request: LlmRequest) -> Result<LlmStream, LlmError> {
		info!("Starting Vertex streaming completion request");

		let vertex_request = VertexRequest::from(&request);

		let response = self
			.send_request(&vertex_request, true)
			.await
			.map_err(LlmError::from)?;

		debug!("Vertex stream connection established");
		let stream = parse_vertex_stream(response.bytes_stream());

		Ok(LlmStream::new(Box::pin(stream)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Verifies client creation succeeds with valid configuration.
	/// Important for ensuring the client can be instantiated without panics.
	#[test]
	fn client_creation_succeeds() {
		let config = VertexConfig::new("test-project", "us-central1");
		let client = VertexClient::new(config);
		assert!(client.is_ok());
	}

	/// Verifies URL construction for non-streaming requests.
	/// Important for ensuring requests go to the correct endpoint.
	#[test]
	fn generate_content_url_non_streaming() {
		let config = VertexConfig::new("my-project", "us-central1").with_model("gemini-1.5-pro");
		let client = VertexClient::new(config).unwrap();
		let url = client.generate_content_url(false);
		assert_eq!(
			url,
			"https://us-central1-aiplatform.googleapis.com/v1/projects/my-project/locations/us-central1/publishers/google/models/gemini-1.5-pro:generateContent"
		);
	}

	/// Verifies URL construction for streaming requests.
	/// Important for ensuring streaming requests use streamGenerateContent endpoint.
	#[test]
	fn generate_content_url_streaming() {
		let config = VertexConfig::new("my-project", "europe-west1").with_model("gemini-2.0-flash");
		let client = VertexClient::new(config).unwrap();
		let url = client.generate_content_url(true);
		assert_eq!(
			url,
			"https://europe-west1-aiplatform.googleapis.com/v1/projects/my-project/locations/europe-west1/publishers/google/models/gemini-2.0-flash:streamGenerateContent"
		);
	}

	/// Verifies custom base URL is used in URL construction.
	/// Important for testing against local mock servers.
	#[test]
	fn custom_base_url() {
		let config = VertexConfig::new("proj", "us-west1").with_base_url("http://localhost:8080");
		let client = VertexClient::new(config).unwrap();
		let url = client.generate_content_url(false);
		assert!(url.starts_with("http://localhost:8080"));
	}
}
