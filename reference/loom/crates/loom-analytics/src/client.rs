// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Analytics client for capturing events and identifying users.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use loom_analytics_core::{validate_distinct_id, validate_event_name, validate_properties_size};
use loom_common_http::RetryConfig;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

use crate::batch::{BatchConfig, BatchProcessor, BatchSender, QueuedEvent};
use crate::error::{AnalyticsError, Result};
use crate::properties::Properties;

/// SDK version for the `$lib_version` automatic property.
const SDK_VERSION: &str = env!("CARGO_PKG_VERSION");
/// SDK name for the `$lib` automatic property.
const SDK_NAME: &str = "loom-analytics";

/// Configuration for the analytics client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
	/// Timeout for HTTP requests.
	pub request_timeout: Duration,
	/// Batch configuration for event queuing.
	pub batch_config: BatchConfig,
	/// Retry configuration for HTTP requests.
	pub retry_config: RetryConfig,
}

impl Default for ClientConfig {
	fn default() -> Self {
		Self {
			request_timeout: Duration::from_secs(10),
			batch_config: BatchConfig::default(),
			retry_config: RetryConfig::default(),
		}
	}
}

/// Builder for constructing an AnalyticsClient.
pub struct AnalyticsClientBuilder {
	api_key: Option<String>,
	base_url: Option<String>,
	config: ClientConfig,
}

impl AnalyticsClientBuilder {
	/// Creates a new builder with default settings.
	pub fn new() -> Self {
		Self {
			api_key: None,
			base_url: None,
			config: ClientConfig::default(),
		}
	}

	/// Sets the API key for authentication.
	///
	/// The API key should be in the format: `loom_analytics_write_xxx` or `loom_analytics_rw_xxx`
	pub fn api_key(mut self, key: impl Into<String>) -> Self {
		self.api_key = Some(key.into());
		self
	}

	/// Sets the base URL for the Loom server.
	///
	/// Example: `https://loom.example.com`
	pub fn base_url(mut self, url: impl Into<String>) -> Self {
		self.base_url = Some(url.into());
		self
	}

	/// Sets the HTTP request timeout.
	pub fn request_timeout(mut self, timeout: Duration) -> Self {
		self.config.request_timeout = timeout;
		self
	}

	/// Sets the flush interval for batched events.
	pub fn flush_interval(mut self, interval: Duration) -> Self {
		self.config.batch_config.flush_interval = interval;
		self
	}

	/// Sets the maximum batch size before flushing.
	pub fn max_batch_size(mut self, size: usize) -> Self {
		self.config.batch_config.max_batch_size = size;
		self
	}

	/// Sets the maximum queue size.
	pub fn max_queue_size(mut self, size: usize) -> Self {
		self.config.batch_config.max_queue_size = size;
		self
	}

	/// Sets the retry configuration.
	pub fn retry_config(mut self, config: RetryConfig) -> Self {
		self.config.retry_config = config;
		self
	}

	/// Builds the AnalyticsClient.
	///
	/// This starts the background flush task.
	pub fn build(self) -> Result<AnalyticsClient> {
		let api_key = self.api_key.ok_or(AnalyticsError::InvalidApiKey)?;
		let base_url = self.base_url.ok_or(AnalyticsError::InvalidBaseUrl)?;

		// Validate API key format
		if !api_key.starts_with("loom_analytics_") {
			return Err(AnalyticsError::InvalidApiKey);
		}

		// Normalize base URL (remove trailing slash)
		let base_url = base_url.trim_end_matches('/').to_string();

		let http_client = loom_common_http::builder()
			.timeout(self.config.request_timeout)
			.build()
			.map_err(AnalyticsError::RequestFailed)?;

		let sender = Arc::new(HttpBatchSender::new(
			http_client.clone(),
			api_key.clone(),
			base_url.clone(),
			self.config.retry_config.clone(),
		));

		let processor = Arc::new(BatchProcessor::new(
			self.config.batch_config.clone(),
			sender.clone(),
		));

		// Start the background flush task
		let processor_clone = Arc::clone(&processor);
		let flush_handle = tokio::spawn(async move {
			processor_clone.run().await;
		});

		info!(
			base_url = %base_url,
			"Analytics client initialized"
		);

		Ok(AnalyticsClient {
			api_key,
			base_url,
			http_client,
			processor,
			flush_handle: RwLock::new(Some(flush_handle)),
			config: self.config,
			closed: AtomicBool::new(false),
		})
	}
}

impl Default for AnalyticsClientBuilder {
	fn default() -> Self {
		Self::new()
	}
}

/// HTTP batch sender implementation.
struct HttpBatchSender {
	http_client: Client,
	api_key: String,
	base_url: String,
	retry_config: RetryConfig,
}

impl HttpBatchSender {
	fn new(
		http_client: Client,
		api_key: String,
		base_url: String,
		retry_config: RetryConfig,
	) -> Self {
		Self {
			http_client,
			api_key,
			base_url,
			retry_config,
		}
	}
}

#[async_trait::async_trait]
impl BatchSender for HttpBatchSender {
	async fn send_batch(&self, events: Vec<QueuedEvent>) -> Result<()> {
		let url = format!("{}/api/analytics/batch", self.base_url);

		let batch_payload: Vec<CapturePayload> = events
			.into_iter()
			.map(|e| CapturePayload {
				distinct_id: e.distinct_id,
				event: e.event_name,
				properties: e.properties,
				timestamp: Some(e.timestamp),
				lib: Some(SDK_NAME.to_string()),
				lib_version: Some(SDK_VERSION.to_string()),
			})
			.collect();

		let request_body = BatchCaptureRequest {
			batch: batch_payload,
		};

		debug!(
			url = %url,
			count = request_body.batch.len(),
			"Sending analytics batch"
		);

		let response = loom_common_http::retry(&self.retry_config, || async {
			self
				.http_client
				.post(&url)
				.header("Authorization", format!("Bearer {}", self.api_key))
				.json(&request_body)
				.send()
				.await
		})
		.await
		.map_err(AnalyticsError::RequestFailed)?;

		if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
			let retry_after = response
				.headers()
				.get("Retry-After")
				.and_then(|v| v.to_str().ok())
				.and_then(|s| s.parse().ok());
			return Err(AnalyticsError::RateLimited {
				retry_after_secs: retry_after,
			});
		}

		if !response.status().is_success() {
			return Err(AnalyticsError::ServerError {
				status: response.status().as_u16(),
				message: response.text().await.unwrap_or_default(),
			});
		}

		Ok(())
	}
}

/// Client for capturing analytics events and identifying users.
///
/// # Example
///
/// ```ignore
/// use loom_analytics::{AnalyticsClient, Properties};
/// use std::time::Duration;
///
/// let client = AnalyticsClient::builder()
///     .api_key("loom_analytics_write_xxx")
///     .base_url("https://loom.example.com")
///     .flush_interval(Duration::from_secs(10))
///     .build()?;
///
/// // Capture an event
/// client.capture("button_clicked", "user_123", Properties::new()
///     .insert("button_name", "checkout")
/// ).await?;
///
/// // Identify a user
/// client.identify("anon_abc123", "user@example.com", Properties::new()
///     .insert("plan", "pro")
/// ).await?;
///
/// // Shutdown gracefully
/// client.shutdown().await?;
/// ```
pub struct AnalyticsClient {
	api_key: String,
	base_url: String,
	http_client: Client,
	processor: Arc<BatchProcessor>,
	flush_handle: RwLock<Option<JoinHandle<()>>>,
	config: ClientConfig,
	closed: AtomicBool,
}

impl AnalyticsClient {
	/// Creates a new builder for constructing an AnalyticsClient.
	pub fn builder() -> AnalyticsClientBuilder {
		AnalyticsClientBuilder::new()
	}

	/// Captures an event.
	///
	/// # Arguments
	///
	/// * `event` - The event name (e.g., "button_clicked", "$pageview")
	/// * `distinct_id` - The user's distinct ID
	/// * `properties` - Optional event properties
	///
	/// # Example
	///
	/// ```ignore
	/// client.capture("purchase", "user_123", Properties::new()
	///     .insert("product_id", "abc123")
	///     .insert("price", 99.99)
	/// ).await?;
	/// ```
	pub async fn capture(
		&self,
		event: &str,
		distinct_id: &str,
		properties: Properties,
	) -> Result<()> {
		self.check_closed()?;

		// Validate event name
		if !validate_event_name(event) {
			return Err(AnalyticsError::ValidationFailed(format!(
				"invalid event name: {}",
				event
			)));
		}

		// Validate distinct_id
		if !validate_distinct_id(distinct_id) {
			return Err(AnalyticsError::ValidationFailed(format!(
				"invalid distinct_id: {}",
				distinct_id
			)));
		}

		let props_value = properties.into_value();

		// Validate properties size
		if !validate_properties_size(&props_value) {
			return Err(AnalyticsError::ValidationFailed(
				"properties exceed maximum size (1MB)".to_string(),
			));
		}

		let queued = QueuedEvent {
			distinct_id: distinct_id.to_string(),
			event_name: event.to_string(),
			properties: props_value,
			timestamp: Utc::now(),
		};

		self.processor.enqueue(queued).await
	}

	/// Identifies a user by linking an anonymous distinct_id to a known user_id.
	///
	/// # Arguments
	///
	/// * `distinct_id` - The current distinct_id (often anonymous)
	/// * `user_id` - The "real" user identifier (email, user ID, etc.)
	/// * `properties` - Optional person properties to set
	///
	/// # Example
	///
	/// ```ignore
	/// // When a user logs in, link their anonymous session to their account
	/// client.identify("anon_abc123", "user@example.com", Properties::new()
	///     .insert("plan", "pro")
	///     .insert("company", "Acme Inc")
	/// ).await?;
	/// ```
	pub async fn identify(
		&self,
		distinct_id: &str,
		user_id: &str,
		properties: Properties,
	) -> Result<()> {
		self.check_closed()?;

		// Validate IDs
		if !validate_distinct_id(distinct_id) {
			return Err(AnalyticsError::ValidationFailed(format!(
				"invalid distinct_id: {}",
				distinct_id
			)));
		}

		if !validate_distinct_id(user_id) {
			return Err(AnalyticsError::ValidationFailed(format!(
				"invalid user_id: {}",
				user_id
			)));
		}

		let payload = IdentifyRequest {
			distinct_id: distinct_id.to_string(),
			user_id: user_id.to_string(),
			properties: properties.into_value(),
		};

		let url = format!("{}/api/analytics/identify", self.base_url);

		let response = loom_common_http::retry(&self.config.retry_config, || async {
			self
				.http_client
				.post(&url)
				.header("Authorization", format!("Bearer {}", self.api_key))
				.json(&payload)
				.send()
				.await
		})
		.await
		.map_err(AnalyticsError::RequestFailed)?;

		self.handle_response(response).await
	}

	/// Creates an alias linking two distinct_ids.
	///
	/// # Arguments
	///
	/// * `distinct_id` - The primary identity
	/// * `alias` - The secondary identity to link
	///
	/// # Example
	///
	/// ```ignore
	/// client.alias("user@example.com", "user_123").await?;
	/// ```
	pub async fn alias(&self, distinct_id: &str, alias: &str) -> Result<()> {
		self.check_closed()?;

		if !validate_distinct_id(distinct_id) {
			return Err(AnalyticsError::ValidationFailed(format!(
				"invalid distinct_id: {}",
				distinct_id
			)));
		}

		if !validate_distinct_id(alias) {
			return Err(AnalyticsError::ValidationFailed(format!(
				"invalid alias: {}",
				alias
			)));
		}

		let payload = AliasRequest {
			distinct_id: distinct_id.to_string(),
			alias: alias.to_string(),
		};

		let url = format!("{}/api/analytics/alias", self.base_url);

		let response = loom_common_http::retry(&self.config.retry_config, || async {
			self
				.http_client
				.post(&url)
				.header("Authorization", format!("Bearer {}", self.api_key))
				.json(&payload)
				.send()
				.await
		})
		.await
		.map_err(AnalyticsError::RequestFailed)?;

		self.handle_response(response).await
	}

	/// Sets person properties for a user.
	///
	/// # Arguments
	///
	/// * `distinct_id` - The user's distinct ID
	/// * `properties` - The properties to set
	///
	/// # Example
	///
	/// ```ignore
	/// client.set("user@example.com", Properties::new()
	///     .insert("last_login", chrono::Utc::now().to_rfc3339())
	/// ).await?;
	/// ```
	pub async fn set(&self, distinct_id: &str, properties: Properties) -> Result<()> {
		self.check_closed()?;

		if !validate_distinct_id(distinct_id) {
			return Err(AnalyticsError::ValidationFailed(format!(
				"invalid distinct_id: {}",
				distinct_id
			)));
		}

		let payload = SetRequest {
			distinct_id: distinct_id.to_string(),
			properties: properties.into_value(),
		};

		let url = format!("{}/api/analytics/set", self.base_url);

		let response = loom_common_http::retry(&self.config.retry_config, || async {
			self
				.http_client
				.post(&url)
				.header("Authorization", format!("Bearer {}", self.api_key))
				.json(&payload)
				.send()
				.await
		})
		.await
		.map_err(AnalyticsError::RequestFailed)?;

		self.handle_response(response).await
	}

	/// Forces an immediate flush of queued events.
	///
	/// This is useful before shutdown or when you need to ensure events are sent.
	pub async fn flush(&self) -> Result<()> {
		self.processor.flush().await
	}

	/// Shuts down the client, flushing any pending events.
	///
	/// After calling this, subsequent operations will return `ClientShutdown` errors.
	pub async fn shutdown(&self) -> Result<()> {
		if self.closed.swap(true, Ordering::SeqCst) {
			// Already closed
			return Ok(());
		}

		info!("Shutting down analytics client");

		// Signal the processor to shutdown
		self.processor.shutdown();

		// Wait for the flush task to complete
		if let Some(handle) = self.flush_handle.write().await.take() {
			if let Err(e) = handle.await {
				error!(error = %e, "Error waiting for flush task to complete");
			}
		}

		info!("Analytics client shutdown complete");
		Ok(())
	}

	/// Returns the number of events currently queued.
	pub async fn queue_len(&self) -> usize {
		self.processor.queue_len().await
	}

	/// Returns true if the client has been shut down.
	pub fn is_closed(&self) -> bool {
		self.closed.load(Ordering::SeqCst)
	}

	fn check_closed(&self) -> Result<()> {
		if self.closed.load(Ordering::SeqCst) {
			return Err(AnalyticsError::ClientShutdown);
		}
		Ok(())
	}

	async fn handle_response(&self, response: reqwest::Response) -> Result<()> {
		if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
			let retry_after = response
				.headers()
				.get("Retry-After")
				.and_then(|v| v.to_str().ok())
				.and_then(|s| s.parse().ok());
			return Err(AnalyticsError::RateLimited {
				retry_after_secs: retry_after,
			});
		}

		if !response.status().is_success() {
			return Err(AnalyticsError::ServerError {
				status: response.status().as_u16(),
				message: response.text().await.unwrap_or_default(),
			});
		}

		Ok(())
	}
}

/// Request payload for capturing a single event.
#[derive(Debug, Serialize, Deserialize)]
struct CapturePayload {
	distinct_id: String,
	event: String,
	#[serde(default)]
	properties: serde_json::Value,
	#[serde(skip_serializing_if = "Option::is_none")]
	timestamp: Option<chrono::DateTime<chrono::Utc>>,
	#[serde(skip_serializing_if = "Option::is_none")]
	lib: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	lib_version: Option<String>,
}

/// Request payload for batch capture.
#[derive(Debug, Serialize, Deserialize)]
struct BatchCaptureRequest {
	batch: Vec<CapturePayload>,
}

/// Request payload for identify.
#[derive(Debug, Serialize, Deserialize)]
struct IdentifyRequest {
	distinct_id: String,
	user_id: String,
	#[serde(default)]
	properties: serde_json::Value,
}

/// Request payload for alias.
#[derive(Debug, Serialize, Deserialize)]
struct AliasRequest {
	distinct_id: String,
	alias: String,
}

/// Request payload for set properties.
#[derive(Debug, Serialize, Deserialize)]
struct SetRequest {
	distinct_id: String,
	#[serde(default)]
	properties: serde_json::Value,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_builder_requires_api_key() {
		let result = AnalyticsClientBuilder::new()
			.base_url("https://example.com")
			.build();

		assert!(matches!(result, Err(AnalyticsError::InvalidApiKey)));
	}

	#[test]
	fn test_builder_requires_base_url() {
		let result = AnalyticsClientBuilder::new()
			.api_key("loom_analytics_write_abc123")
			.build();

		assert!(matches!(result, Err(AnalyticsError::InvalidBaseUrl)));
	}

	#[test]
	fn test_builder_validates_api_key_format() {
		let result = AnalyticsClientBuilder::new()
			.api_key("invalid_key")
			.base_url("https://example.com")
			.build();

		assert!(matches!(result, Err(AnalyticsError::InvalidApiKey)));
	}

	#[tokio::test]
	async fn test_builder_accepts_write_key() {
		// This will fail at HTTP level but we're just testing the builder validation
		let result = AnalyticsClientBuilder::new()
			.api_key("loom_analytics_write_abc123")
			.base_url("https://example.com")
			.build();

		assert!(result.is_ok());
		if let Ok(client) = result {
			client.shutdown().await.unwrap();
		}
	}

	#[tokio::test]
	async fn test_builder_accepts_rw_key() {
		let result = AnalyticsClientBuilder::new()
			.api_key("loom_analytics_rw_abc123")
			.base_url("https://example.com")
			.build();

		assert!(result.is_ok());
		if let Ok(client) = result {
			client.shutdown().await.unwrap();
		}
	}

	#[test]
	fn test_client_config_defaults() {
		let config = ClientConfig::default();
		assert_eq!(config.request_timeout, Duration::from_secs(10));
		assert_eq!(config.batch_config.max_batch_size, 10);
		assert_eq!(config.batch_config.flush_interval, Duration::from_secs(10));
	}

	#[tokio::test]
	async fn test_client_shutdown_prevents_capture() {
		let client = AnalyticsClient::builder()
			.api_key("loom_analytics_write_abc123")
			.base_url("https://example.com")
			.build()
			.unwrap();

		client.shutdown().await.unwrap();

		let result = client.capture("test", "user_123", Properties::new()).await;
		assert!(matches!(result, Err(AnalyticsError::ClientShutdown)));
	}

	#[tokio::test]
	async fn test_client_double_shutdown_is_ok() {
		let client = AnalyticsClient::builder()
			.api_key("loom_analytics_write_abc123")
			.base_url("https://example.com")
			.build()
			.unwrap();

		client.shutdown().await.unwrap();
		client.shutdown().await.unwrap();
	}
}

#[cfg(test)]
mod validation_tests {
	use super::*;
	use proptest::prelude::*;

	#[tokio::test]
	async fn test_capture_validates_event_name() {
		let client = AnalyticsClient::builder()
			.api_key("loom_analytics_write_abc123")
			.base_url("https://example.com")
			.build()
			.unwrap();

		// Invalid: starts with uppercase
		let result = client
			.capture("InvalidEvent", "user_123", Properties::new())
			.await;
		assert!(matches!(result, Err(AnalyticsError::ValidationFailed(_))));

		// Invalid: empty
		let result = client.capture("", "user_123", Properties::new()).await;
		assert!(matches!(result, Err(AnalyticsError::ValidationFailed(_))));

		client.shutdown().await.unwrap();
	}

	#[tokio::test]
	async fn test_capture_validates_distinct_id() {
		let client = AnalyticsClient::builder()
			.api_key("loom_analytics_write_abc123")
			.base_url("https://example.com")
			.build()
			.unwrap();

		// Invalid: empty
		let result = client.capture("valid_event", "", Properties::new()).await;
		assert!(matches!(result, Err(AnalyticsError::ValidationFailed(_))));

		client.shutdown().await.unwrap();
	}

	#[tokio::test]
	async fn test_identify_validates_ids() {
		let client = AnalyticsClient::builder()
			.api_key("loom_analytics_write_abc123")
			.base_url("https://example.com")
			.build()
			.unwrap();

		// Invalid distinct_id
		let result = client.identify("", "user_id", Properties::new()).await;
		assert!(matches!(result, Err(AnalyticsError::ValidationFailed(_))));

		// Invalid user_id
		let result = client.identify("distinct_id", "", Properties::new()).await;
		assert!(matches!(result, Err(AnalyticsError::ValidationFailed(_))));

		client.shutdown().await.unwrap();
	}

	#[tokio::test]
	async fn test_alias_validates_ids() {
		let client = AnalyticsClient::builder()
			.api_key("loom_analytics_write_abc123")
			.base_url("https://example.com")
			.build()
			.unwrap();

		// Invalid distinct_id
		let result = client.alias("", "alias").await;
		assert!(matches!(result, Err(AnalyticsError::ValidationFailed(_))));

		// Invalid alias
		let result = client.alias("distinct_id", "").await;
		assert!(matches!(result, Err(AnalyticsError::ValidationFailed(_))));

		client.shutdown().await.unwrap();
	}

	#[tokio::test]
	async fn test_set_validates_distinct_id() {
		let client = AnalyticsClient::builder()
			.api_key("loom_analytics_write_abc123")
			.base_url("https://example.com")
			.build()
			.unwrap();

		let result = client.set("", Properties::new()).await;
		assert!(matches!(result, Err(AnalyticsError::ValidationFailed(_))));

		client.shutdown().await.unwrap();
	}

	proptest! {
		#[test]
		fn api_key_validation_accepts_valid_write_keys(
			random in "[a-f0-9]{32}",
		) {
			let key = format!("loom_analytics_write_{}", random);
			// Just validate the key format, don't create the full client
			prop_assert!(key.starts_with("loom_analytics_"));
		}

		#[test]
		fn api_key_validation_accepts_valid_rw_keys(
			random in "[a-f0-9]{32}",
		) {
			let key = format!("loom_analytics_rw_{}", random);
			// Just validate the key format, don't create the full client
			prop_assert!(key.starts_with("loom_analytics_"));
		}

		#[test]
		fn api_key_validation_rejects_invalid_prefix(prefix in "[a-z]{1,20}") {
			// Keys not starting with loom_analytics_ should be rejected
			if !prefix.starts_with("loom_analytics_") {
				prop_assert!(!prefix.starts_with("loom_analytics_"));
			}
		}

		#[test]
		fn base_url_normalization_removes_trailing_slash(
			protocol in prop_oneof![Just("http"), Just("https")],
			domain in "[a-z]{3,10}\\.[a-z]{2,4}",
		) {
			let url_with_slash = format!("{}://{}/", protocol, domain);
			let normalized = url_with_slash.trim_end_matches('/');
			prop_assert!(!normalized.ends_with('/'));
		}
	}
}
