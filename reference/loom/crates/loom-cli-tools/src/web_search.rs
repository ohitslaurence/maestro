// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Web search tool that proxies requests through the Loom server.

use async_trait::async_trait;
use loom_common_core::{ToolContext, ToolError};
use loom_common_http::{retry, RetryConfig};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::Tool;

#[derive(Debug, Deserialize)]
struct WebSearchArgs {
	query: String,
	max_results: Option<u32>,
}

#[derive(Debug, Serialize)]
struct WebSearchRequest {
	query: String,
	max_results: u32,
}

pub struct WebSearchToolGoogle {
	client: Client,
	base_url: String,
}

impl WebSearchToolGoogle {
	pub fn new(base_url: impl Into<String>) -> Self {
		Self {
			client: loom_common_http::new_client(),
			base_url: base_url.into(),
		}
	}
}

impl Default for WebSearchToolGoogle {
	fn default() -> Self {
		let base_url =
			std::env::var("LOOM_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
		Self::new(base_url)
	}
}

#[async_trait]
impl Tool for WebSearchToolGoogle {
	fn name(&self) -> &str {
		"web_search"
	}

	fn description(&self) -> &str {
		"Perform a web search via the Loom server using Google Custom Search Engine (CSE)."
	}

	fn input_schema(&self) -> serde_json::Value {
		serde_json::json!({
				"type": "object",
				"properties": {
						"query": {
								"type": "string",
								"description": "Search query string in natural language."
						},
						"max_results": {
								"type": "integer",
								"minimum": 1,
								"maximum": 10,
								"description": "Maximum number of search results to return (default: 5, max: 10)."
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
		let args: WebSearchArgs =
			serde_json::from_value(args).map_err(|e| ToolError::Serialization(e.to_string()))?;

		let query = args.query.trim().to_string();
		if query.is_empty() {
			return Err(ToolError::InvalidArguments(
				"query must not be empty".to_string(),
			));
		}

		let max_results = args.max_results.unwrap_or(5).min(10);

		tracing::debug!(
				query = %query,
				max_results = max_results,
				"web_search: sending request to server proxy"
		);

		let url = format!("{}/proxy/cse", self.base_url.trim_end_matches('/'));

		let request_body = WebSearchRequest {
			query: query.clone(),
			max_results,
		};

		let retry_config = RetryConfig::default();
		let response = retry(&retry_config, || {
			let client = &self.client;
			let url = &url;
			let request_body = &request_body;
			async move {
				client
					.post(url)
					.json(request_body)
					.timeout(std::time::Duration::from_secs(30))
					.send()
					.await
			}
		})
		.await
		.map_err(|e| {
			if e.is_timeout() {
				tracing::warn!(error = %e, "web_search: server timeout");
				ToolError::Timeout
			} else {
				tracing::error!(error = %e, "web_search: network error");
				ToolError::Io(e.to_string())
			}
		})?;

		let status = response.status();
		let body = response
			.text()
			.await
			.map_err(|e| ToolError::Io(e.to_string()))?;

		if !status.is_success() {
			tracing::warn!(
					status = %status,
					body = %body,
					"web_search: server returned non-success"
			);
			return Err(ToolError::Internal(format!(
				"web_search proxy error: HTTP {status}"
			)));
		}

		let value: serde_json::Value =
			serde_json::from_str(&body).map_err(|e| ToolError::Serialization(e.to_string()))?;

		tracing::debug!(
				query = %query,
				results_count = ?value.get("results").and_then(|r| r.as_array()).map(|a| a.len()),
				"web_search: received response"
		);

		Ok(value)
	}
}

pub struct WebSearchToolSerper {
	client: Client,
	base_url: String,
}

impl WebSearchToolSerper {
	pub fn new(base_url: impl Into<String>) -> Self {
		Self {
			client: loom_common_http::new_client(),
			base_url: base_url.into(),
		}
	}
}

impl Default for WebSearchToolSerper {
	fn default() -> Self {
		let base_url =
			std::env::var("LOOM_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string());
		Self::new(base_url)
	}
}

#[async_trait]
impl Tool for WebSearchToolSerper {
	fn name(&self) -> &str {
		"web_search_serper"
	}

	fn description(&self) -> &str {
		"Perform a web search via the Loom server using Serper.dev API."
	}

	fn input_schema(&self) -> serde_json::Value {
		serde_json::json!({
				"type": "object",
				"properties": {
						"query": {
								"type": "string",
								"description": "Search query string in natural language."
						},
						"max_results": {
								"type": "integer",
								"minimum": 1,
								"maximum": 100,
								"description": "Maximum number of search results to return (default: 5, max: 100)."
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
		let args: WebSearchArgs =
			serde_json::from_value(args).map_err(|e| ToolError::Serialization(e.to_string()))?;

		let query = args.query.trim().to_string();
		if query.is_empty() {
			return Err(ToolError::InvalidArguments(
				"query must not be empty".to_string(),
			));
		}

		let max_results = args.max_results.unwrap_or(5).min(100);

		tracing::debug!(
				query = %query,
				max_results = max_results,
				"web_search_serper: sending request to server proxy"
		);

		let url = format!("{}/proxy/serper", self.base_url.trim_end_matches('/'));

		let request_body = WebSearchRequest {
			query: query.clone(),
			max_results,
		};

		let retry_config = RetryConfig::default();
		let response = retry(&retry_config, || {
			let client = &self.client;
			let url = &url;
			let request_body = &request_body;
			async move {
				client
					.post(url)
					.json(request_body)
					.timeout(std::time::Duration::from_secs(30))
					.send()
					.await
			}
		})
		.await
		.map_err(|e| {
			if e.is_timeout() {
				tracing::warn!(error = %e, "web_search_serper: server timeout");
				ToolError::Timeout
			} else {
				tracing::error!(error = %e, "web_search_serper: network error");
				ToolError::Io(e.to_string())
			}
		})?;

		let status = response.status();
		let body = response
			.text()
			.await
			.map_err(|e| ToolError::Io(e.to_string()))?;

		if !status.is_success() {
			tracing::warn!(
					status = %status,
					body = %body,
					"web_search_serper: server returned non-success"
			);
			return Err(ToolError::Internal(format!(
				"web_search_serper proxy error: HTTP {status}"
			)));
		}

		let value: serde_json::Value =
			serde_json::from_str(&body).map_err(|e| ToolError::Serialization(e.to_string()))?;

		tracing::debug!(
				query = %query,
				results_count = ?value.get("results").and_then(|r| r.as_array()).map(|a| a.len()),
				"web_search_serper: received response"
		);

		Ok(value)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	#[tokio::test]
	async fn test_google_empty_query_returns_error() {
		let tool = WebSearchToolGoogle::new("http://localhost:8080");
		let ctx = ToolContext {
			workspace_root: PathBuf::from("/tmp"),
		};

		let result = tool.invoke(serde_json::json!({"query": ""}), &ctx).await;
		assert!(matches!(result, Err(ToolError::InvalidArguments(_))));

		let result = tool.invoke(serde_json::json!({"query": "   "}), &ctx).await;
		assert!(matches!(result, Err(ToolError::InvalidArguments(_))));
	}

	#[tokio::test]
	async fn test_google_whitespace_only_query_returns_error() {
		let tool = WebSearchToolGoogle::new("http://localhost:8080");
		let ctx = ToolContext {
			workspace_root: PathBuf::from("/tmp"),
		};

		let result = tool
			.invoke(serde_json::json!({"query": "\t\n  "}), &ctx)
			.await;
		assert!(matches!(result, Err(ToolError::InvalidArguments(_))));
	}

	#[tokio::test]
	async fn test_serper_empty_query_returns_error() {
		let tool = WebSearchToolSerper::new("http://localhost:8080");
		let ctx = ToolContext {
			workspace_root: PathBuf::from("/tmp"),
		};

		let result = tool.invoke(serde_json::json!({"query": ""}), &ctx).await;
		assert!(matches!(result, Err(ToolError::InvalidArguments(_))));

		let result = tool.invoke(serde_json::json!({"query": "   "}), &ctx).await;
		assert!(matches!(result, Err(ToolError::InvalidArguments(_))));
	}

	#[tokio::test]
	async fn test_serper_whitespace_only_query_returns_error() {
		let tool = WebSearchToolSerper::new("http://localhost:8080");
		let ctx = ToolContext {
			workspace_root: PathBuf::from("/tmp"),
		};

		let result = tool
			.invoke(serde_json::json!({"query": "\t\n  "}), &ctx)
			.await;
		assert!(matches!(result, Err(ToolError::InvalidArguments(_))));
	}
}
