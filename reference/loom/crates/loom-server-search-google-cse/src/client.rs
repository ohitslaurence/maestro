// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Google Custom Search Engine client implementation.

use std::time::Duration;

use loom_common_http::{retry, RetryConfig};
use reqwest::{Client, Url};
use serde::Deserialize;
use tracing::{debug, error, instrument, trace};

use crate::error::CseError;
use crate::types::{CseRequest, CseResponse, CseResultItem};

const DEFAULT_BASE_URL: &str = "https://www.googleapis.com/customsearch/v1";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Client for interacting with Google Custom Search Engine API.
#[derive(Debug, Clone)]
pub struct CseClient {
	http_client: Client,
	api_key: String,
	cx: String,
	base_url: String,
	retry_config: RetryConfig,
}

#[derive(Debug, Deserialize)]
struct GoogleCseResponse {
	items: Option<Vec<GoogleCseItem>>,
	error: Option<GoogleCseError>,
}

#[derive(Debug, Deserialize)]
struct GoogleCseItem {
	title: String,
	link: String,
	snippet: Option<String>,
	#[serde(rename = "displayLink")]
	display_link: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleCseError {
	code: u16,
	message: String,
}

impl CseClient {
	/// Creates a new CSE client with the given API key and search engine ID.
	pub fn new(api_key: impl Into<String>, cx: impl Into<String>) -> Self {
		let http_client = loom_common_http::builder()
			.timeout(REQUEST_TIMEOUT)
			.build()
			.expect("Failed to create HTTP client");

		Self {
			http_client,
			api_key: api_key.into(),
			cx: cx.into(),
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

	/// Performs a search using the Google Custom Search Engine API.
	#[instrument(skip(self), fields(query = %request.query, num = request.num))]
	pub async fn search(&self, request: CseRequest) -> Result<CseResponse, CseError> {
		let query = request.query.clone();
		let num = request.num;

		retry(&self.retry_config, || self.search_inner(&query, num)).await
	}

	async fn search_inner(&self, query: &str, num: u32) -> Result<CseResponse, CseError> {
		let mut url = Url::parse(&self.base_url)
			.map_err(|e| CseError::InvalidResponse(format!("Invalid base URL: {e}")))?;

		url
			.query_pairs_mut()
			.append_pair("key", &self.api_key)
			.append_pair("cx", &self.cx)
			.append_pair("q", query)
			.append_pair("num", &num.to_string());

		debug!(url = %self.base_url, "Sending search request to Google CSE");
		trace!(query = %query, num = num, "Search parameters");

		let response = self.http_client.get(url).send().await.map_err(|e| {
			if e.is_timeout() {
				error!("Request timed out");
				return CseError::Timeout;
			}
			error!(error = %e, "Network error during CSE request");
			CseError::Network(e)
		})?;

		let status = response.status();
		debug!(status = %status, "Received response from Google CSE");

		if !status.is_success() {
			let status_code = status.as_u16();
			let body = response.text().await.unwrap_or_default();

			if status_code == 401 || status_code == 403 {
				if body.to_lowercase().contains("rate")
					|| body.to_lowercase().contains("quota")
					|| body.to_lowercase().contains("limit")
				{
					error!(status = status_code, "Rate limit exceeded");
					return Err(CseError::RateLimited);
				}
				error!(status = status_code, "Unauthorized request");
				return Err(CseError::Unauthorized);
			}

			error!(status = status_code, body = %body, "Google API error");
			return Err(CseError::ApiError {
				status: status_code,
				message: body,
			});
		}

		let body = response.text().await.map_err(|e| {
			error!(error = %e, "Failed to read response body");
			CseError::Network(e)
		})?;

		trace!(body = %body, "Response body");

		let google_response: GoogleCseResponse = serde_json::from_str(&body).map_err(|e| {
			error!(error = %e, "Failed to parse Google CSE response");
			CseError::InvalidResponse(format!("JSON parse error: {e}"))
		})?;

		if let Some(error) = google_response.error {
			error!(code = error.code, message = %error.message, "Google API returned error");
			return Err(CseError::ApiError {
				status: error.code,
				message: error.message,
			});
		}

		let results: Vec<CseResultItem> = google_response
			.items
			.unwrap_or_default()
			.into_iter()
			.enumerate()
			.map(|(index, item)| CseResultItem {
				title: item.title,
				url: item.link,
				snippet: item.snippet.unwrap_or_default(),
				display_link: item.display_link,
				rank: (index + 1) as u32,
			})
			.collect();

		debug!(
			result_count = results.len(),
			"Search completed successfully"
		);

		Ok(CseResponse {
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
		let client = CseClient::new("test-api-key", "test-cx");
		assert_eq!(client.api_key, "test-api-key");
		assert_eq!(client.cx, "test-cx");
		assert_eq!(client.base_url, DEFAULT_BASE_URL);
	}

	#[test]
	fn test_with_base_url() {
		let client = CseClient::new("key", "cx").with_base_url("https://custom.api.com");
		assert_eq!(client.base_url, "https://custom.api.com");
	}
}
