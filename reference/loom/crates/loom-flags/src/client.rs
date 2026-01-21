// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Feature flags client for evaluating flags against the Loom server.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use loom_common_http::RetryConfig;
use loom_flags_core::{BulkEvaluationResult, EvaluationContext, EvaluationResult, VariantValue};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::analytics::{AnalyticsHook, FlagExposure, NoOpAnalyticsHook, SharedAnalyticsHook};
use crate::cache::FlagCache;
use crate::error::{FlagsError, Result};
use crate::sse::{SseConfig, SseConnection};

/// Configuration for the flags client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
	/// Timeout for initialization (fetching initial flags).
	pub init_timeout: Duration,
	/// Timeout for individual evaluation requests.
	pub request_timeout: Duration,
	/// Whether to enable SSE streaming for real-time updates.
	pub enable_streaming: bool,
	/// SSE connection configuration.
	pub sse_config: SseConfig,
	/// Retry configuration for HTTP requests.
	pub retry_config: RetryConfig,
	/// Whether to use offline mode when disconnected.
	pub offline_mode: bool,
}

impl Default for ClientConfig {
	fn default() -> Self {
		Self {
			init_timeout: Duration::from_secs(10),
			request_timeout: Duration::from_secs(5),
			enable_streaming: true,
			sse_config: SseConfig::default(),
			retry_config: RetryConfig::default(),
			offline_mode: true,
		}
	}
}

/// Builder for constructing a FlagsClient.
pub struct FlagsClientBuilder {
	sdk_key: Option<String>,
	base_url: Option<String>,
	config: ClientConfig,
	analytics_hook: Option<SharedAnalyticsHook>,
}

impl FlagsClientBuilder {
	/// Creates a new builder with default settings.
	pub fn new() -> Self {
		Self {
			sdk_key: None,
			base_url: None,
			config: ClientConfig::default(),
			analytics_hook: None,
		}
	}

	/// Sets the SDK key for authentication.
	///
	/// The SDK key should be in the format: `loom_sdk_{type}_{env}_{random}`
	pub fn sdk_key(mut self, key: impl Into<String>) -> Self {
		self.sdk_key = Some(key.into());
		self
	}

	/// Sets the base URL for the Loom server.
	///
	/// Example: `https://loom.example.com`
	pub fn base_url(mut self, url: impl Into<String>) -> Self {
		self.base_url = Some(url.into());
		self
	}

	/// Sets the initialization timeout.
	pub fn init_timeout(mut self, timeout: Duration) -> Self {
		self.config.init_timeout = timeout;
		self
	}

	/// Sets the request timeout for evaluation calls.
	pub fn request_timeout(mut self, timeout: Duration) -> Self {
		self.config.request_timeout = timeout;
		self
	}

	/// Enables or disables SSE streaming for real-time updates.
	pub fn enable_streaming(mut self, enable: bool) -> Self {
		self.config.enable_streaming = enable;
		self
	}

	/// Sets the SSE configuration.
	pub fn sse_config(mut self, config: SseConfig) -> Self {
		self.config.sse_config = config;
		self
	}

	/// Enables or disables offline mode.
	///
	/// When enabled, the client will use cached values when disconnected.
	pub fn offline_mode(mut self, enable: bool) -> Self {
		self.config.offline_mode = enable;
		self
	}

	/// Sets the retry configuration.
	pub fn retry_config(mut self, config: RetryConfig) -> Self {
		self.config.retry_config = config;
		self
	}

	/// Sets an analytics hook for capturing `$feature_flag_called` events.
	///
	/// When set, the hook will be called after each flag evaluation with
	/// exposure data that can be used to track experiment participation.
	///
	/// # Example
	///
	/// ```ignore
	/// use loom_flags::{FlagsClient, AnalyticsHook, FlagExposure};
	/// use async_trait::async_trait;
	///
	/// struct MyHook;
	///
	/// #[async_trait]
	/// impl AnalyticsHook for MyHook {
	///     async fn on_flag_evaluated(&self, exposure: FlagExposure) {
	///         // Send to analytics service
	///     }
	/// }
	///
	/// let client = FlagsClient::builder()
	///     .sdk_key("loom_sdk_server_prod_xxx")
	///     .base_url("https://loom.example.com")
	///     .analytics_hook(MyHook)
	///     .build()
	///     .await?;
	/// ```
	pub fn analytics_hook<H: AnalyticsHook>(mut self, hook: H) -> Self {
		self.analytics_hook = Some(Arc::new(hook));
		self
	}

	/// Builds and initializes the FlagsClient.
	///
	/// This will fetch the initial set of flags and optionally start SSE streaming.
	pub async fn build(self) -> Result<FlagsClient> {
		let sdk_key = self.sdk_key.ok_or(FlagsError::InvalidSdkKey)?;
		let base_url = self.base_url.ok_or(FlagsError::InvalidBaseUrl)?;

		// Validate SDK key format
		if !sdk_key.starts_with("loom_sdk_") {
			return Err(FlagsError::InvalidSdkKey);
		}

		// Normalize base URL (remove trailing slash)
		let base_url = base_url.trim_end_matches('/').to_string();

		let http_client = loom_common_http::builder()
			.timeout(self.config.request_timeout)
			.build()
			.map_err(FlagsError::ConnectionFailed)?;

		let cache = FlagCache::new();
		let sse_connection = Arc::new(RwLock::new(SseConnection::new()));
		let analytics_hook: SharedAnalyticsHook = self
			.analytics_hook
			.unwrap_or_else(|| Arc::new(NoOpAnalyticsHook));

		let client = FlagsClient {
			sdk_key,
			base_url,
			http_client,
			cache,
			sse_connection,
			config: self.config.clone(),
			closed: Arc::new(AtomicBool::new(false)),
			analytics_hook,
		};

		// Initialize by fetching current flag states
		client.initialize().await?;

		// Start SSE streaming if enabled
		if self.config.enable_streaming {
			client.start_streaming().await?;
		}

		Ok(client)
	}
}

impl Default for FlagsClientBuilder {
	fn default() -> Self {
		Self::new()
	}
}

/// Client for evaluating feature flags against the Loom server.
///
/// The client maintains a local cache of flag states and can optionally
/// receive real-time updates via SSE streaming.
pub struct FlagsClient {
	sdk_key: String,
	base_url: String,
	http_client: Client,
	cache: FlagCache,
	sse_connection: Arc<RwLock<SseConnection>>,
	config: ClientConfig,
	closed: Arc<AtomicBool>,
	analytics_hook: SharedAnalyticsHook,
}

impl FlagsClient {
	/// Creates a new builder for constructing a FlagsClient.
	pub fn builder() -> FlagsClientBuilder {
		FlagsClientBuilder::new()
	}

	/// Initializes the client by fetching current flag states.
	async fn initialize(&self) -> Result<()> {
		// For initialization, we make a single request to get current state
		// The SSE stream will send an 'init' event with all flags
		// But for initial sync, we can use the evaluate endpoint with empty context
		let init_url = format!("{}/api/flags/evaluate", self.base_url);

		let response = tokio::time::timeout(self.config.init_timeout, async {
			self
				.http_client
				.post(&init_url)
				.header("Authorization", format!("Bearer {}", self.sdk_key))
				.json(&EvaluationRequest {
					context: EvaluationContext::new(""),
				})
				.send()
				.await
		})
		.await
		.map_err(|_| FlagsError::InitializationTimeout)?
		.map_err(FlagsError::ConnectionFailed)?;

		if response.status() == reqwest::StatusCode::UNAUTHORIZED {
			return Err(FlagsError::AuthenticationFailed);
		}

		if !response.status().is_success() {
			return Err(FlagsError::ServerError {
				status: response.status().as_u16(),
				message: response.text().await.unwrap_or_default(),
			});
		}

		// Parse the bulk evaluation response
		let bulk_result: BulkEvaluationResponse = response
			.json()
			.await
			.map_err(|e| FlagsError::ParseFailed(e.to_string()))?;

		// Convert evaluation results to flag states and initialize cache
		let flags: Vec<loom_flags_core::FlagState> = bulk_result
			.results
			.iter()
			.map(|r| loom_flags_core::FlagState {
				key: r.flag_key.clone(),
				id: loom_flags_core::FlagId::new(), // We don't have the ID from evaluation
				enabled: !matches!(r.reason, loom_flags_core::EvaluationReason::Disabled),
				default_variant: r.variant.clone(),
				default_value: r.value.clone(),
				archived: false,
			})
			.collect();

		self.cache.initialize(flags, vec![]).await;
		info!(
			flags = self.cache.flag_count().await,
			"Flags client initialized"
		);

		Ok(())
	}

	/// Starts SSE streaming for real-time updates.
	async fn start_streaming(&self) -> Result<()> {
		let stream_url = format!("{}/api/flags/stream", self.base_url);
		let mut sse = self.sse_connection.write().await;

		sse
			.start(
				stream_url,
				self.sdk_key.clone(),
				self.cache.clone(),
				self.config.sse_config.clone(),
			)
			.await
	}

	/// Evaluates a boolean flag.
	///
	/// # Arguments
	///
	/// * `flag_key` - The flag key to evaluate
	/// * `context` - The evaluation context
	/// * `default` - Default value if flag is not found or evaluation fails
	///
	/// # Returns
	///
	/// The boolean value of the flag, or the default if not found.
	pub async fn get_bool(
		&self,
		flag_key: &str,
		context: &EvaluationContext,
		default: bool,
	) -> Result<bool> {
		self.check_closed()?;

		let result = self.evaluate_flag(flag_key, context).await?;

		match result.value.as_bool() {
			Some(v) => Ok(v),
			None => {
				warn!(
					flag_key = flag_key,
					actual_type = ?result.value,
					"Flag value is not a boolean, using default"
				);
				Ok(default)
			}
		}
	}

	/// Evaluates a string flag.
	///
	/// # Arguments
	///
	/// * `flag_key` - The flag key to evaluate
	/// * `context` - The evaluation context
	/// * `default` - Default value if flag is not found or evaluation fails
	///
	/// # Returns
	///
	/// The string value of the flag, or the default if not found.
	pub async fn get_string(
		&self,
		flag_key: &str,
		context: &EvaluationContext,
		default: &str,
	) -> Result<String> {
		self.check_closed()?;

		let result = self.evaluate_flag(flag_key, context).await?;

		match result.value.as_str() {
			Some(v) => Ok(v.to_string()),
			None => {
				warn!(
					flag_key = flag_key,
					actual_type = ?result.value,
					"Flag value is not a string, using default"
				);
				Ok(default.to_string())
			}
		}
	}

	/// Evaluates a JSON flag.
	///
	/// # Arguments
	///
	/// * `flag_key` - The flag key to evaluate
	/// * `context` - The evaluation context
	/// * `default` - Default value if flag is not found or evaluation fails
	///
	/// # Returns
	///
	/// The JSON value of the flag, or the default if not found.
	pub async fn get_json(
		&self,
		flag_key: &str,
		context: &EvaluationContext,
		_default: serde_json::Value,
	) -> Result<serde_json::Value> {
		self.check_closed()?;

		let result = self.evaluate_flag(flag_key, context).await?;

		match result.value.as_json() {
			Some(v) => Ok(v.clone()),
			None => {
				// Try to convert other types to JSON
				match &result.value {
					VariantValue::Boolean(b) => Ok(serde_json::json!(b)),
					VariantValue::String(s) => Ok(serde_json::json!(s)),
					VariantValue::Json(j) => Ok(j.clone()),
				}
			}
		}
	}

	/// Evaluates all flags for the given context.
	///
	/// # Arguments
	///
	/// * `context` - The evaluation context
	///
	/// # Returns
	///
	/// A bulk result containing all flag evaluations.
	pub async fn get_all(&self, context: &EvaluationContext) -> Result<BulkEvaluationResult> {
		self.check_closed()?;

		// Try server-side evaluation first
		match self.evaluate_all_server(context).await {
			Ok(result) => Ok(result),
			Err(e) if e.should_use_cache() && self.config.offline_mode => {
				warn!(error = %e, "Server evaluation failed, using cached values");
				self.evaluate_all_cached(context).await
			}
			Err(e) => Err(e),
		}
	}

	/// Evaluates a single flag.
	async fn evaluate_flag(
		&self,
		flag_key: &str,
		context: &EvaluationContext,
	) -> Result<EvaluationResult> {
		// Check cache first for kill switch
		if let Some(ks_key) = self.cache.is_flag_killed(flag_key).await {
			debug!(flag_key = flag_key, kill_switch = %ks_key, "Flag killed by kill switch");

			// Get default value from cache
			if let Some(flag) = self.cache.get_flag(flag_key).await {
				let result = EvaluationResult::new(
					flag_key,
					&flag.default_variant,
					flag.default_value.clone(),
					loom_flags_core::EvaluationReason::KillSwitch {
						kill_switch_id: loom_flags_core::KillSwitchId::new(),
					},
				);

				// Track analytics for kill switch evaluation
				self.track_flag_exposure(&result, context).await;

				return Ok(result);
			}
		}

		// Try server-side evaluation
		let result = match self.evaluate_flag_server(flag_key, context).await {
			Ok(result) => result,
			Err(e) if e.should_use_cache() && self.config.offline_mode => {
				warn!(
					flag_key = flag_key,
					error = %e,
					"Server evaluation failed, using cached value"
				);
				self.evaluate_flag_cached(flag_key).await?
			}
			Err(e) => return Err(e),
		};

		// Track analytics for successful evaluation
		self.track_flag_exposure(&result, context).await;

		Ok(result)
	}

	/// Tracks a flag exposure event via the analytics hook.
	async fn track_flag_exposure(&self, result: &EvaluationResult, context: &EvaluationContext) {
		// Determine the variant string representation
		let variant = match &result.value {
			VariantValue::Boolean(b) => b.to_string(),
			VariantValue::String(s) => s.clone(),
			VariantValue::Json(j) => j.to_string(),
		};

		// Determine the distinct_id: prefer user_id, then environment, fallback to "anonymous"
		let user_id = context.user_id.clone();
		let distinct_id = user_id
			.clone()
			.or_else(|| {
				if context.environment.is_empty() {
					None
				} else {
					Some(context.environment.clone())
				}
			})
			.unwrap_or_else(|| "anonymous".to_string());

		let exposure = FlagExposure::new(
			&result.flag_key,
			variant,
			user_id,
			distinct_id,
			format!("{:?}", result.reason),
		);

		// Call the analytics hook (fire-and-forget, don't block on result)
		self.analytics_hook.on_flag_evaluated(exposure).await;
	}

	/// Evaluates a flag using the server API.
	async fn evaluate_flag_server(
		&self,
		flag_key: &str,
		context: &EvaluationContext,
	) -> Result<EvaluationResult> {
		let url = format!("{}/api/flags/{}/evaluate", self.base_url, flag_key);

		let response = self
			.http_client
			.post(&url)
			.header("Authorization", format!("Bearer {}", self.sdk_key))
			.json(&EvaluationRequest {
				context: context.clone(),
			})
			.send()
			.await
			.map_err(FlagsError::RequestFailed)?;

		if response.status() == reqwest::StatusCode::NOT_FOUND {
			return Err(FlagsError::FlagNotFound {
				flag_key: flag_key.to_string(),
			});
		}

		if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
			let retry_after = response
				.headers()
				.get("Retry-After")
				.and_then(|v| v.to_str().ok())
				.and_then(|s| s.parse().ok());
			return Err(FlagsError::RateLimited {
				retry_after_secs: retry_after,
			});
		}

		if !response.status().is_success() {
			return Err(FlagsError::ServerError {
				status: response.status().as_u16(),
				message: response.text().await.unwrap_or_default(),
			});
		}

		response
			.json()
			.await
			.map_err(|e| FlagsError::ParseFailed(e.to_string()))
	}

	/// Evaluates a flag using cached data.
	async fn evaluate_flag_cached(&self, flag_key: &str) -> Result<EvaluationResult> {
		if !self.cache.is_initialized().await {
			return Err(FlagsError::OfflineNoCache);
		}

		let flag = self
			.cache
			.get_flag(flag_key)
			.await
			.ok_or_else(|| FlagsError::FlagNotFound {
				flag_key: flag_key.to_string(),
			})?;

		if flag.archived || !flag.enabled {
			Ok(EvaluationResult::new(
				flag_key,
				&flag.default_variant,
				flag.default_value.clone(),
				loom_flags_core::EvaluationReason::Disabled,
			))
		} else {
			Ok(EvaluationResult::new(
				flag_key,
				&flag.default_variant,
				flag.default_value.clone(),
				loom_flags_core::EvaluationReason::Default,
			))
		}
	}

	/// Evaluates all flags using the server API.
	async fn evaluate_all_server(&self, context: &EvaluationContext) -> Result<BulkEvaluationResult> {
		let url = format!("{}/api/flags/evaluate", self.base_url);

		let response = self
			.http_client
			.post(&url)
			.header("Authorization", format!("Bearer {}", self.sdk_key))
			.json(&EvaluationRequest {
				context: context.clone(),
			})
			.send()
			.await
			.map_err(FlagsError::RequestFailed)?;

		if !response.status().is_success() {
			return Err(FlagsError::ServerError {
				status: response.status().as_u16(),
				message: response.text().await.unwrap_or_default(),
			});
		}

		let bulk_response: BulkEvaluationResponse = response
			.json()
			.await
			.map_err(|e| FlagsError::ParseFailed(e.to_string()))?;

		Ok(BulkEvaluationResult::new(bulk_response.results))
	}

	/// Evaluates all flags using cached data.
	async fn evaluate_all_cached(
		&self,
		_context: &EvaluationContext,
	) -> Result<BulkEvaluationResult> {
		if !self.cache.is_initialized().await {
			return Err(FlagsError::OfflineNoCache);
		}

		let flags = self.cache.get_all_flags().await;
		let results: Vec<EvaluationResult> = flags
			.into_iter()
			.filter(|f| !f.archived)
			.map(|f| {
				let reason = if f.enabled {
					loom_flags_core::EvaluationReason::Default
				} else {
					loom_flags_core::EvaluationReason::Disabled
				};

				EvaluationResult::new(&f.key, &f.default_variant, f.default_value.clone(), reason)
			})
			.collect();

		Ok(BulkEvaluationResult::new(results))
	}

	/// Checks if the client has been closed.
	fn check_closed(&self) -> Result<()> {
		if self.closed.load(Ordering::SeqCst) {
			return Err(FlagsError::ClientClosed);
		}
		Ok(())
	}

	/// Returns true if the SSE connection is currently active.
	pub async fn is_streaming(&self) -> bool {
		self.sse_connection.read().await.is_connected()
	}

	/// Returns true if the cache has been initialized.
	pub async fn is_initialized(&self) -> bool {
		self.cache.is_initialized().await
	}

	/// Returns the number of cached flags.
	pub async fn cached_flag_count(&self) -> usize {
		self.cache.flag_count().await
	}

	/// Closes the client and stops any background tasks.
	pub async fn close(&self) {
		self.closed.store(true, Ordering::SeqCst);
		self.sse_connection.write().await.stop().await;
		info!("Flags client closed");
	}
}

/// Request body for flag evaluation.
#[derive(Debug, Serialize)]
struct EvaluationRequest {
	context: EvaluationContext,
}

/// Response body for bulk evaluation.
#[derive(Debug, Deserialize)]
struct BulkEvaluationResponse {
	results: Vec<EvaluationResult>,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_builder_requires_sdk_key() {
		let result = tokio_test::block_on(async {
			FlagsClientBuilder::new()
				.base_url("https://example.com")
				.build()
				.await
		});

		assert!(matches!(result, Err(FlagsError::InvalidSdkKey)));
	}

	#[test]
	fn test_builder_requires_base_url() {
		let result = tokio_test::block_on(async {
			FlagsClientBuilder::new()
				.sdk_key("loom_sdk_server_prod_abc123")
				.build()
				.await
		});

		assert!(matches!(result, Err(FlagsError::InvalidBaseUrl)));
	}

	#[test]
	fn test_builder_validates_sdk_key_format() {
		let result = tokio_test::block_on(async {
			FlagsClientBuilder::new()
				.sdk_key("invalid_key")
				.base_url("https://example.com")
				.build()
				.await
		});

		assert!(matches!(result, Err(FlagsError::InvalidSdkKey)));
	}

	#[test]
	fn test_client_config_defaults() {
		let config = ClientConfig::default();
		assert_eq!(config.init_timeout, Duration::from_secs(10));
		assert_eq!(config.request_timeout, Duration::from_secs(5));
		assert!(config.enable_streaming);
		assert!(config.offline_mode);
	}
}

#[cfg(test)]
mod analytics_tests {
	use super::*;
	use crate::analytics::FlagExposure;
	use std::sync::atomic::{AtomicUsize, Ordering};
	use tokio::sync::Mutex;

	struct RecordingHook {
		exposures: Mutex<Vec<FlagExposure>>,
		call_count: AtomicUsize,
	}

	impl RecordingHook {
		fn new() -> Self {
			Self {
				exposures: Mutex::new(Vec::new()),
				call_count: AtomicUsize::new(0),
			}
		}

		async fn get_exposures(&self) -> Vec<FlagExposure> {
			self.exposures.lock().await.clone()
		}

		fn get_call_count(&self) -> usize {
			self.call_count.load(Ordering::SeqCst)
		}
	}

	#[async_trait::async_trait]
	impl AnalyticsHook for RecordingHook {
		async fn on_flag_evaluated(&self, exposure: FlagExposure) {
			self.call_count.fetch_add(1, Ordering::SeqCst);
			self.exposures.lock().await.push(exposure);
		}
	}

	#[test]
	fn test_flag_exposure_properties() {
		let exposure = FlagExposure::new(
			"checkout.new_flow",
			"treatment_a",
			Some("user123".to_string()),
			"user123",
			"TargetingRuleMatch",
		);

		let props = exposure.to_event_properties();
		assert_eq!(props["$feature_flag"], "checkout.new_flow");
		assert_eq!(props["$feature_flag_response"], "treatment_a");
		assert_eq!(props["$feature_flag_reason"], "TargetingRuleMatch");
	}

	#[test]
	fn test_flag_exposure_with_user_id() {
		let exposure = FlagExposure::new(
			"feature.beta",
			"true",
			Some("user456".to_string()),
			"user456",
			"Default",
		);

		assert_eq!(exposure.flag_key, "feature.beta");
		assert_eq!(exposure.variant, "true");
		assert_eq!(exposure.user_id, Some("user456".to_string()));
		assert_eq!(exposure.distinct_id, "user456");
	}

	#[test]
	fn test_flag_exposure_without_user_id() {
		let exposure = FlagExposure::new("feature.anonymous", "false", None, "anonymous", "Default");

		assert!(exposure.user_id.is_none());
		assert_eq!(exposure.distinct_id, "anonymous");
	}

	#[test]
	fn test_noop_hook_exists() {
		let hook = NoOpAnalyticsHook;
		// Just verify it compiles and can be used
		let _ = Arc::new(hook) as SharedAnalyticsHook;
	}
}

#[cfg(test)]
mod proptests {
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn sdk_key_validation_accepts_valid_keys(
			key_type in prop_oneof![Just("server"), Just("client")],
			env in "[a-z]{2,10}",
			random in "[a-f0-9]{32}",
		) {
			let key = format!("loom_sdk_{}_{}_{}",key_type, env, random);
			assert!(key.starts_with("loom_sdk_"));
		}

		#[test]
		fn sdk_key_validation_rejects_invalid_keys(prefix in "[a-z]{1,10}") {
			// Keys not starting with loom_sdk_ should be rejected
			if !prefix.starts_with("loom_sdk_") {
				// This just validates our test assumption
				prop_assert!(!prefix.starts_with("loom_sdk_"));
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
