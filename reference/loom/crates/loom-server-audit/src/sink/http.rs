// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

#![cfg(feature = "sink-http")]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loom_server_config::HttpSinkConfig;
use rand::Rng;
use reqwest::{Client, Method, StatusCode};

use crate::enrichment::EnrichedAuditEvent;
use crate::error::AuditSinkError;
use crate::filter::AuditFilterConfig;
use crate::sink::AuditSink;

/// HTTP audit sink for sending events to external SIEM/logging systems.
///
/// # Security Note
///
/// Header values may contain secrets (API keys, tokens). The config struct
/// intentionally does not derive Debug to avoid accidental logging of secrets.
/// When logging errors, never include the raw header values.
pub struct HttpAuditSink {
	config: HttpSinkConfig,
	filter: AuditFilterConfig,
	client: Client,
}

impl HttpAuditSink {
	pub fn new(config: HttpSinkConfig, filter: AuditFilterConfig) -> Result<Self, AuditSinkError> {
		validate_config(&config)?;

		let client = loom_common_http::builder()
			.timeout(Duration::from_millis(config.timeout_ms))
			.build()
			.map_err(|e| AuditSinkError::Permanent(format!("failed to create HTTP client: {e}")))?;

		Ok(Self {
			config,
			filter,
			client,
		})
	}

	fn parse_method(&self) -> Method {
		match self.config.method.to_uppercase().as_str() {
			"PUT" => Method::PUT,
			"PATCH" => Method::PATCH,
			_ => Method::POST,
		}
	}

	async fn send_with_retry(&self, body: &str) -> Result<(), AuditSinkError> {
		let method = self.parse_method();
		let mut last_error = None;

		for attempt in 0..self.config.retry_max_attempts {
			if attempt > 0 {
				// Exponential backoff with jitter to prevent timing attacks and thundering herd
				let base_backoff_ms = 100 * 2u64.pow(attempt - 1);
				let jitter_ms = rand::rng().random_range(0..=base_backoff_ms / 2);
				let backoff = Duration::from_millis(base_backoff_ms + jitter_ms);
				tokio::time::sleep(backoff).await;
			}

			let mut request = self.client.request(method.clone(), &self.config.url);

			request = request.header("Content-Type", "application/json");

			for (key, value) in &self.config.headers {
				request = request.header(key, value);
			}

			request = request.body(body.to_string());

			match request.send().await {
				Ok(response) => {
					let status = response.status();
					if status.is_success() {
						return Ok(());
					}

					if is_permanent_status(status) {
						// Truncate response body to avoid logging potentially sensitive data
						// reflected back from our request
						let body = response.text().await.unwrap_or_default();
						let truncated_body: String = body.chars().take(200).collect();
						return Err(AuditSinkError::Permanent(format!(
							"HTTP {} {}{}",
							status.as_u16(),
							status.canonical_reason().unwrap_or(""),
							if truncated_body.is_empty() {
								String::new()
							} else {
								format!(": {}", truncated_body)
							}
						)));
					}

					last_error = Some(AuditSinkError::Transient(format!(
						"HTTP {} {}",
						status.as_u16(),
						status.canonical_reason().unwrap_or("")
					)));
				}
				Err(e) => {
					last_error = Some(AuditSinkError::Transient(format!("request failed: {e}")));
				}
			}
		}

		Err(last_error.unwrap_or_else(|| AuditSinkError::Transient("unknown error".to_string())))
	}
}

pub fn validate_config(config: &HttpSinkConfig) -> Result<(), AuditSinkError> {
	if config.name.is_empty() {
		return Err(AuditSinkError::Permanent(
			"name cannot be empty".to_string(),
		));
	}
	if config.url.is_empty() {
		return Err(AuditSinkError::Permanent("url cannot be empty".to_string()));
	}
	if !config.url.starts_with("http://") && !config.url.starts_with("https://") {
		return Err(AuditSinkError::Permanent(
			"url must start with http:// or https://".to_string(),
		));
	}
	let method_upper = config.method.to_uppercase();
	if !["POST", "PUT", "PATCH"].contains(&method_upper.as_str()) {
		return Err(AuditSinkError::Permanent(format!(
			"unsupported method: {}",
			config.method
		)));
	}
	if config.timeout_ms == 0 {
		return Err(AuditSinkError::Permanent(
			"timeout_ms must be greater than 0".to_string(),
		));
	}
	Ok(())
}

fn is_permanent_status(status: StatusCode) -> bool {
	matches!(
		status,
		StatusCode::BAD_REQUEST
			| StatusCode::UNAUTHORIZED
			| StatusCode::FORBIDDEN
			| StatusCode::NOT_FOUND
			| StatusCode::METHOD_NOT_ALLOWED
			| StatusCode::NOT_ACCEPTABLE
			| StatusCode::GONE
			| StatusCode::UNSUPPORTED_MEDIA_TYPE
			| StatusCode::UNPROCESSABLE_ENTITY
	)
}

#[async_trait]
impl AuditSink for HttpAuditSink {
	fn name(&self) -> &str {
		&self.config.name
	}

	fn filter(&self) -> &AuditFilterConfig {
		&self.filter
	}

	async fn publish(&self, event: Arc<EnrichedAuditEvent>) -> Result<(), AuditSinkError> {
		let body = serde_json::to_string(&*event)
			.map_err(|e| AuditSinkError::Permanent(format!("failed to serialize event: {e}")))?;

		self.send_with_retry(&body).await
	}

	async fn health_check(&self) -> Result<(), AuditSinkError> {
		let response = self
			.client
			.head(&self.config.url)
			.send()
			.await
			.map_err(|e| AuditSinkError::Transient(format!("health check failed: {e}")))?;

		if response.status().is_server_error() {
			return Err(AuditSinkError::Transient(format!(
				"health check returned {}",
				response.status()
			)));
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn default_filter() -> AuditFilterConfig {
		AuditFilterConfig::default()
	}

	fn make_config(name: &str, url: &str) -> HttpSinkConfig {
		HttpSinkConfig {
			name: name.to_string(),
			url: url.to_string(),
			method: "POST".to_string(),
			headers: vec![],
			timeout_ms: 5000,
			retry_max_attempts: 3,
			min_severity: "info".to_string(),
		}
	}

	#[test]
	fn test_config_valid() {
		let config = HttpSinkConfig {
			name: "datadog".to_string(),
			url: "https://http-intake.logs.datadoghq.com/api/v2/logs".to_string(),
			method: "POST".to_string(),
			headers: vec![("DD-API-KEY".to_string(), "secret".to_string())],
			timeout_ms: 5000,
			retry_max_attempts: 3,
			min_severity: "info".to_string(),
		};
		assert!(validate_config(&config).is_ok());
	}

	#[test]
	fn test_config_empty_name() {
		let config = make_config("", "https://example.com");
		let err = validate_config(&config).unwrap_err();
		assert!(err.to_string().contains("name cannot be empty"));
	}

	#[test]
	fn test_config_empty_url() {
		let config = make_config("test", "");
		let err = validate_config(&config).unwrap_err();
		assert!(err.to_string().contains("url cannot be empty"));
	}

	#[test]
	fn test_config_invalid_url_scheme() {
		let config = make_config("test", "ftp://example.com");
		let err = validate_config(&config).unwrap_err();
		assert!(err.to_string().contains("http://"));
	}

	#[test]
	fn test_config_invalid_method() {
		let mut config = make_config("test", "https://example.com");
		config.method = "DELETE".to_string();
		let err = validate_config(&config).unwrap_err();
		assert!(err.to_string().contains("unsupported method"));
	}

	#[test]
	fn test_config_zero_timeout() {
		let mut config = make_config("test", "https://example.com");
		config.timeout_ms = 0;
		let err = validate_config(&config).unwrap_err();
		assert!(err.to_string().contains("timeout_ms"));
	}

	#[test]
	fn test_sink_creation_valid() {
		let config = HttpSinkConfig {
			name: "splunk".to_string(),
			url: "https://splunk-hec.example.com:8088/services/collector/event".to_string(),
			method: "POST".to_string(),
			headers: vec![("Authorization".to_string(), "Splunk token".to_string())],
			timeout_ms: 5000,
			retry_max_attempts: 3,
			min_severity: "info".to_string(),
		};
		let sink = HttpAuditSink::new(config, default_filter());
		assert!(sink.is_ok());
	}

	#[test]
	fn test_sink_creation_invalid_config() {
		let config = make_config("", "https://example.com");
		let sink = HttpAuditSink::new(config, default_filter());
		assert!(sink.is_err());
	}

	#[test]
	fn test_is_permanent_status() {
		assert!(is_permanent_status(StatusCode::BAD_REQUEST));
		assert!(is_permanent_status(StatusCode::UNAUTHORIZED));
		assert!(is_permanent_status(StatusCode::FORBIDDEN));
		assert!(is_permanent_status(StatusCode::NOT_FOUND));
		assert!(!is_permanent_status(StatusCode::OK));
		assert!(!is_permanent_status(StatusCode::INTERNAL_SERVER_ERROR));
		assert!(!is_permanent_status(StatusCode::BAD_GATEWAY));
		assert!(!is_permanent_status(StatusCode::SERVICE_UNAVAILABLE));
	}
}
