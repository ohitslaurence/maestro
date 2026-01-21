// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Serper.dev API client implementation.

use std::time::Duration;

use loom_common_http::{retry, RetryConfig};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, instrument, trace};

use crate::error::SerperError;
use crate::types::{SerperRequest, SerperResponse, SerperResultItem};

const DEFAULT_BASE_URL: &str = "https://google.serper.dev/search";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Client for interacting with Serper.dev Google Search API.
#[derive(Debug, Clone)]
pub struct SerperClient {
	http_client: Client,
	api_key: String,
	base_url: String,
	retry_config: RetryConfig,
}

#[derive(Debug, Serialize)]
struct SerperApiRequest {
	q: String,
	num: u32,
}

#[derive(Debug, Deserialize)]
struct SerperApiResponse {
	organic: Option<Vec<SerperOrganicItem>>,
}

#[derive(Debug, Deserialize)]
struct SerperOrganicItem {
	title: String,
	link: String,
	snippet: Option<String>,
	position: u32,
}

impl SerperClient {
	/// Creates a new Serper client with the given API key.
	pub fn new(api_key: impl Into<String>) -> Self {
		let http_client = loom_common_http::builder()
			.timeout(REQUEST_TIMEOUT)
			.build()
			.expect("Failed to create HTTP client");

		Self {
			http_client,
			api_key: api_key.into(),
			base_url: DEFAULT_BASE_URL.to_string(),
			retry_config: RetryConfig::default(),
		}
	}

	/// Sets a custom base URL for the API (useful for testing).
	pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
		self.base_url = base_url.into();
		self
	}

	/// Sets a custom retry configuration.
	pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
		self.retry_config = config;
		self
	}

	/// Performs a search using the Serper.dev API.
	#[instrument(skip(self), fields(query = %request.query, num = request.num))]
	pub async fn search(&self, request: SerperRequest) -> Result<SerperResponse, SerperError> {
		let query = request.query.clone();
		let num = request.num;

		retry(&self.retry_config, || self.search_inner(&query, num)).await
	}

	async fn search_inner(&self, query: &str, num: u32) -> Result<SerperResponse, SerperError> {
		let api_request = SerperApiRequest {
			q: query.to_string(),
			num,
		};

		debug!(url = %self.base_url, "Sending search request to Serper");
		trace!(query = %query, num = num, "Search parameters");

		let response = self
			.http_client
			.post(&self.base_url)
			.header("X-API-KEY", &self.api_key)
			.json(&api_request)
			.send()
			.await
			.map_err(|e| {
				if e.is_timeout() {
					error!("Request timed out");
					return SerperError::Timeout;
				}
				error!(error = %e, "Network error during Serper request");
				SerperError::Network(e)
			})?;

		let status = response.status();
		debug!(status = %status, "Received response from Serper");

		if !status.is_success() {
			let status_code = status.as_u16();
			let body = response.text().await.unwrap_or_default();

			if status_code == 401 || status_code == 403 {
				if body.to_lowercase().contains("rate")
					|| body.to_lowercase().contains("quota")
					|| body.to_lowercase().contains("limit")
				{
					error!(status = status_code, "Rate limit exceeded");
					return Err(SerperError::RateLimited);
				}
				error!(status = status_code, "Unauthorized request");
				return Err(SerperError::Unauthorized);
			}

			if status_code == 429 {
				error!(status = status_code, "Rate limit exceeded");
				return Err(SerperError::RateLimited);
			}

			error!(status = status_code, body = %body, "Serper API error");
			return Err(SerperError::ApiError {
				status: status_code,
				message: body,
			});
		}

		let body = response.text().await.map_err(|e| {
			error!(error = %e, "Failed to read response body");
			SerperError::Network(e)
		})?;

		trace!(body = %body, "Response body");

		let serper_response: SerperApiResponse = serde_json::from_str(&body).map_err(|e| {
			error!(error = %e, "Failed to parse Serper response");
			SerperError::InvalidResponse(format!("JSON parse error: {e}"))
		})?;

		let results: Vec<SerperResultItem> = serper_response
			.organic
			.unwrap_or_default()
			.into_iter()
			.map(|item| SerperResultItem {
				title: item.title,
				url: item.link,
				snippet: item.snippet.unwrap_or_default(),
				position: item.position,
			})
			.collect();

		debug!(
			result_count = results.len(),
			"Search completed successfully"
		);

		Ok(SerperResponse {
			query: query.to_string(),
			results,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_client_creation() {
		let client = SerperClient::new("test-api-key");
		assert_eq!(client.api_key, "test-api-key");
		assert_eq!(client.base_url, DEFAULT_BASE_URL);
	}

	#[test]
	fn test_with_base_url() {
		let client = SerperClient::new("key").with_base_url("https://custom.api.com");
		assert_eq!(client.base_url, "https://custom.api.com");
	}

	#[test]
	fn test_with_retry_config() {
		let config = RetryConfig {
			max_attempts: 5,
			..Default::default()
		};
		let client = SerperClient::new("key").with_retry_config(config);
		assert_eq!(client.retry_config.max_attempts, 5);
	}
}
