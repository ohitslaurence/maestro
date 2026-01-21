// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! HTTP client for LLM proxy communication.

use async_trait::async_trait;
use loom_common_secret::SecretString;
use tracing::{debug, info, instrument};

use loom_common_core::{LlmClient, LlmError, LlmRequest, LlmResponse, LlmStream};

use crate::stream::ProxyLlmStream;
use crate::types::LlmProxyResponse;

/// LLM provider selection for proxy requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LlmProvider {
	#[default]
	Anthropic,
	OpenAi,
	Vertex,
}

impl std::fmt::Display for LlmProvider {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			LlmProvider::Anthropic => write!(f, "anthropic"),
			LlmProvider::OpenAi => write!(f, "openai"),
			LlmProvider::Vertex => write!(f, "vertex"),
		}
	}
}

/// LLM client that communicates with a server proxy instead of direct LLM providers.
///
/// This client sends requests to the server's proxy endpoints, which handle
/// authentication and communication with the actual LLM providers.
pub struct ProxyLlmClient {
	base_url: String,
	provider: LlmProvider,
	http_client: reqwest::Client,
	auth_token: Option<SecretString>,
}

impl ProxyLlmClient {
	/// Creates a new proxy client for Anthropic with the given base URL.
	pub fn anthropic(base_url: impl Into<String>) -> Self {
		Self::new(base_url, LlmProvider::Anthropic)
	}

	/// Creates a new proxy client for OpenAI with the given base URL.
	pub fn openai(base_url: impl Into<String>) -> Self {
		Self::new(base_url, LlmProvider::OpenAi)
	}

	/// Creates a new proxy client for Vertex AI with the given base URL.
	pub fn vertex(base_url: impl Into<String>) -> Self {
		Self::new(base_url, LlmProvider::Vertex)
	}

	/// Creates a new proxy client with the given base URL and provider.
	pub fn new(base_url: impl Into<String>, provider: LlmProvider) -> Self {
		let base_url = base_url.into();
		info!(base_url = %base_url, provider = %provider, "creating ProxyLlmClient");
		Self {
			base_url,
			provider,
			http_client: loom_common_http::new_client(),
			auth_token: None,
		}
	}

	/// Sets an optional authentication token for bearer auth.
	pub fn with_auth_token(mut self, token: SecretString) -> Self {
		self.auth_token = Some(token);
		self
	}

	/// Creates a new proxy client with a custom HTTP client.
	pub fn with_http_client(
		base_url: impl Into<String>,
		provider: LlmProvider,
		http_client: reqwest::Client,
	) -> Self {
		let base_url = base_url.into();
		info!(base_url = %base_url, provider = %provider, "creating ProxyLlmClient with custom HTTP client");
		Self {
			base_url,
			provider,
			http_client,
			auth_token: None,
		}
	}

	/// Returns the configured provider.
	pub fn provider(&self) -> LlmProvider {
		self.provider
	}

	fn complete_url(&self) -> String {
		format!(
			"{}/proxy/{}/complete",
			self.base_url.trim_end_matches('/'),
			self.provider
		)
	}

	fn stream_url(&self) -> String {
		format!(
			"{}/proxy/{}/stream",
			self.base_url.trim_end_matches('/'),
			self.provider
		)
	}
}

#[async_trait]
impl LlmClient for ProxyLlmClient {
	/// Sends a completion request to the proxy and waits for the full response.
	#[instrument(skip(self, request), fields(model = %request.model, provider = %self.provider))]
	async fn complete(&self, request: LlmRequest) -> Result<LlmResponse, LlmError> {
		let url = self.complete_url();
		debug!(url = %url, "sending completion request to proxy");

		let mut req = self.http_client.post(&url).json(&request);
		if let Some(token) = &self.auth_token {
			req = req.bearer_auth(token.expose());
		}

		let response = req.send().await.map_err(|e| {
			debug!(error = %e, "HTTP request failed");
			LlmError::Http(e.to_string())
		})?;

		let status = response.status();
		debug!(status = %status, "received response from proxy");

		if !status.is_success() {
			let error_body = response.text().await.unwrap_or_default();
			debug!(status = %status, body = %error_body, "proxy returned error status");

			if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
				return Err(LlmError::RateLimited {
					retry_after_secs: None,
				});
			}

			return Err(LlmError::Api(format!(
				"proxy returned status {status}: {error_body}"
			)));
		}

		let proxy_response: LlmProxyResponse = response.json().await.map_err(|e| {
			debug!(error = %e, "failed to parse proxy response");
			LlmError::InvalidResponse(format!("failed to parse response: {e}"))
		})?;

		debug!("successfully parsed proxy response");
		Ok(proxy_response.into())
	}

	/// Sends a streaming completion request to the proxy.
	#[instrument(skip(self, request), fields(model = %request.model, provider = %self.provider))]
	async fn complete_streaming(&self, request: LlmRequest) -> Result<LlmStream, LlmError> {
		let url = self.stream_url();
		debug!(url = %url, "sending streaming request to proxy");

		let mut req = self.http_client.post(&url).json(&request);
		if let Some(token) = &self.auth_token {
			req = req.bearer_auth(token.expose());
		}

		let response = req.send().await.map_err(|e| {
			debug!(error = %e, "HTTP request failed");
			LlmError::Http(e.to_string())
		})?;

		let status = response.status();
		debug!(status = %status, "received streaming response from proxy");

		if !status.is_success() {
			let error_body = response.text().await.unwrap_or_default();
			debug!(status = %status, body = %error_body, "proxy returned error status");

			if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
				return Err(LlmError::RateLimited {
					retry_after_secs: None,
				});
			}

			return Err(LlmError::Api(format!(
				"proxy returned status {status}: {error_body}"
			)));
		}

		let byte_stream = response.bytes_stream();
		let proxy_stream = ProxyLlmStream::new(Box::pin(byte_stream));

		debug!("created streaming response");
		Ok(LlmStream::new(Box::pin(proxy_stream)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn constructs_correct_anthropic_urls() {
		let client = ProxyLlmClient::anthropic("http://localhost:8080");
		assert_eq!(
			client.complete_url(),
			"http://localhost:8080/proxy/anthropic/complete"
		);
		assert_eq!(
			client.stream_url(),
			"http://localhost:8080/proxy/anthropic/stream"
		);
	}

	#[test]
	fn constructs_correct_openai_urls() {
		let client = ProxyLlmClient::openai("http://localhost:8080");
		assert_eq!(
			client.complete_url(),
			"http://localhost:8080/proxy/openai/complete"
		);
		assert_eq!(
			client.stream_url(),
			"http://localhost:8080/proxy/openai/stream"
		);
	}

	#[test]
	fn constructs_correct_vertex_urls() {
		let client = ProxyLlmClient::vertex("http://localhost:8080");
		assert_eq!(
			client.complete_url(),
			"http://localhost:8080/proxy/vertex/complete"
		);
		assert_eq!(
			client.stream_url(),
			"http://localhost:8080/proxy/vertex/stream"
		);
	}

	#[test]
	fn handles_trailing_slash_in_base_url() {
		let client = ProxyLlmClient::anthropic("http://localhost:8080/");
		assert_eq!(
			client.complete_url(),
			"http://localhost:8080/proxy/anthropic/complete"
		);
		assert_eq!(
			client.stream_url(),
			"http://localhost:8080/proxy/anthropic/stream"
		);
	}

	#[test]
	fn provider_returns_configured_provider() {
		let anthropic = ProxyLlmClient::anthropic("http://localhost:8080");
		assert_eq!(anthropic.provider(), LlmProvider::Anthropic);

		let openai = ProxyLlmClient::openai("http://localhost:8080");
		assert_eq!(openai.provider(), LlmProvider::OpenAi);

		let vertex = ProxyLlmClient::vertex("http://localhost:8080");
		assert_eq!(vertex.provider(), LlmProvider::Vertex);
	}
}
