// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Analytics integration for feature flag exposure tracking.
//!
//! This module provides the [`AnalyticsHook`] trait for capturing `$feature_flag_called`
//! events when feature flags are evaluated. This enables experiment analysis by connecting
//! feature flag exposures to user behavior analytics.
//!
//! # Overview
//!
//! When a feature flag is evaluated, the client can optionally capture a
//! `$feature_flag_called` event with the following properties:
//!
//! | Property | Description |
//! |----------|-------------|
//! | `$feature_flag` | The flag key that was evaluated |
//! | `$feature_flag_response` | The variant/value returned |
//! | `$feature_flag_reason` | The evaluation reason (e.g., "Default", "TargetingRuleMatch") |
//!
//! # Experiment Analysis
//!
//! Feature flag exposures can be joined with analytics events to measure experiment
//! impact. The exposure `distinct_id` links to `analytics_person_identities.distinct_id`,
//! enabling queries like:
//!
//! ```sql
//! SELECT
//!   el.variant,
//!   COUNT(DISTINCT ae.person_id) as conversions,
//!   COUNT(DISTINCT el.user_id) as exposures,
//!   CAST(COUNT(DISTINCT ae.person_id) AS REAL) / COUNT(DISTINCT el.user_id)
//!     as conversion_rate
//! FROM exposure_logs el
//! LEFT JOIN analytics_person_identities api ON api.distinct_id = el.user_id
//! LEFT JOIN analytics_events ae ON ae.person_id = api.person_id
//!   AND ae.event_name = 'checkout_completed'
//!   AND ae.timestamp > el.timestamp
//! WHERE el.flag_key = 'checkout.new_flow'
//! GROUP BY el.variant;
//! ```
//!
//! # Example
//!
//! ```ignore
//! use loom_flags::{FlagsClient, AnalyticsHook, FlagExposure};
//! use async_trait::async_trait;
//!
//! struct MyAnalyticsHook {
//!     // Your analytics client
//! }
//!
//! #[async_trait]
//! impl AnalyticsHook for MyAnalyticsHook {
//!     async fn on_flag_evaluated(&self, exposure: FlagExposure) {
//!         // Capture $feature_flag_called event
//!         println!("Flag {} = {}", exposure.flag_key, exposure.variant);
//!     }
//! }
//!
//! let client = FlagsClient::builder()
//!     .sdk_key("loom_sdk_server_prod_xxx")
//!     .base_url("https://loom.example.com")
//!     .analytics_hook(MyAnalyticsHook { /* ... */ })
//!     .build()
//!     .await?;
//! ```

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Data captured when a feature flag is evaluated.
///
/// This represents the information needed to track a `$feature_flag_called` event
/// for experiment analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagExposure {
	/// The key of the evaluated flag.
	pub flag_key: String,

	/// The variant/value that was returned.
	///
	/// For boolean flags, this will be "true" or "false".
	/// For string flags, this will be the string value.
	/// For JSON flags, this will be the JSON serialized as a string.
	pub variant: String,

	/// The user ID from the evaluation context, if provided.
	pub user_id: Option<String>,

	/// The distinct_id to use for the analytics event.
	///
	/// This is typically derived from the evaluation context:
	/// - If `user_id` is set, use that
	/// - Otherwise, use the environment from the context
	/// - Fallback to "anonymous"
	pub distinct_id: String,

	/// The reason for the evaluation result.
	pub evaluation_reason: String,

	/// Timestamp of the evaluation.
	pub timestamp: DateTime<Utc>,
}

impl FlagExposure {
	/// Creates a new flag exposure from evaluation data.
	pub fn new(
		flag_key: impl Into<String>,
		variant: impl Into<String>,
		user_id: Option<String>,
		distinct_id: impl Into<String>,
		evaluation_reason: impl Into<String>,
	) -> Self {
		Self {
			flag_key: flag_key.into(),
			variant: variant.into(),
			user_id,
			distinct_id: distinct_id.into(),
			evaluation_reason: evaluation_reason.into(),
			timestamp: Utc::now(),
		}
	}

	/// Converts this exposure to the standard `$feature_flag_called` event properties.
	///
	/// Returns a JSON object with:
	/// - `$feature_flag`: The flag key
	/// - `$feature_flag_response`: The variant value
	/// - `$feature_flag_reason`: The evaluation reason
	pub fn to_event_properties(&self) -> serde_json::Value {
		serde_json::json!({
			"$feature_flag": self.flag_key,
			"$feature_flag_response": self.variant,
			"$feature_flag_reason": self.evaluation_reason,
		})
	}
}

/// Trait for receiving feature flag evaluation events.
///
/// Implement this trait to capture `$feature_flag_called` events when flags are
/// evaluated. This enables experiment analysis by connecting flag exposures to
/// user behavior in analytics.
///
/// The hook is called asynchronously after each flag evaluation. Implementations
/// should be fast and non-blocking; use background queuing for expensive operations
/// like HTTP requests.
#[async_trait]
pub trait AnalyticsHook: Send + Sync + 'static {
	/// Called after a feature flag is evaluated.
	///
	/// # Arguments
	///
	/// * `exposure` - Data about the flag evaluation, including the flag key,
	///   variant returned, user context, and evaluation reason.
	///
	/// # Implementation Notes
	///
	/// This method is called on the evaluation path. Keep it fast by:
	/// - Queueing events for batch sending
	/// - Using a background task for HTTP requests
	/// - Handling errors gracefully (don't let analytics failures break flag evaluation)
	async fn on_flag_evaluated(&self, exposure: FlagExposure);
}

/// Type alias for a shared analytics hook.
pub type SharedAnalyticsHook = Arc<dyn AnalyticsHook>;

/// A no-op analytics hook that discards all events.
///
/// This is used when no analytics integration is configured.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoOpAnalyticsHook;

#[async_trait]
impl AnalyticsHook for NoOpAnalyticsHook {
	async fn on_flag_evaluated(&self, _exposure: FlagExposure) {
		// No-op: discard the event
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::atomic::{AtomicUsize, Ordering};

	#[test]
	fn flag_exposure_new() {
		let exposure = FlagExposure::new(
			"checkout.new_flow",
			"treatment_a",
			Some("user123".to_string()),
			"user123",
			"TargetingRuleMatch",
		);

		assert_eq!(exposure.flag_key, "checkout.new_flow");
		assert_eq!(exposure.variant, "treatment_a");
		assert_eq!(exposure.user_id, Some("user123".to_string()));
		assert_eq!(exposure.distinct_id, "user123");
		assert_eq!(exposure.evaluation_reason, "TargetingRuleMatch");
	}

	#[test]
	fn flag_exposure_to_event_properties() {
		let exposure = FlagExposure::new("feature.beta", "true", None, "anon_12345", "Default");

		let props = exposure.to_event_properties();

		assert_eq!(props["$feature_flag"], "feature.beta");
		assert_eq!(props["$feature_flag_response"], "true");
		assert_eq!(props["$feature_flag_reason"], "Default");
	}

	#[test]
	fn flag_exposure_serialization() {
		let exposure = FlagExposure::new(
			"test.flag",
			"variant_b",
			Some("user456".to_string()),
			"user456",
			"RolloutPercentage",
		);

		let json = serde_json::to_string(&exposure).unwrap();
		let deserialized: FlagExposure = serde_json::from_str(&json).unwrap();

		assert_eq!(deserialized.flag_key, exposure.flag_key);
		assert_eq!(deserialized.variant, exposure.variant);
		assert_eq!(deserialized.user_id, exposure.user_id);
	}

	struct CountingHook {
		count: AtomicUsize,
	}

	#[async_trait]
	impl AnalyticsHook for CountingHook {
		async fn on_flag_evaluated(&self, _exposure: FlagExposure) {
			self.count.fetch_add(1, Ordering::SeqCst);
		}
	}

	#[tokio::test]
	async fn analytics_hook_is_called() {
		let hook = CountingHook {
			count: AtomicUsize::new(0),
		};

		let exposure = FlagExposure::new("test", "true", None, "user", "Default");
		hook.on_flag_evaluated(exposure).await;

		assert_eq!(hook.count.load(Ordering::SeqCst), 1);
	}

	#[tokio::test]
	async fn noop_hook_does_nothing() {
		let hook = NoOpAnalyticsHook;
		let exposure = FlagExposure::new("test", "true", None, "user", "Default");

		// Should not panic or error
		hook.on_flag_evaluated(exposure).await;
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn flag_exposure_roundtrip(
			flag_key in "[a-z][a-z0-9._]{0,50}",
			variant in "[a-zA-Z0-9_]{1,20}",
			user_id in prop::option::of("[a-zA-Z0-9]{5,20}"),
			distinct_id in "[a-zA-Z0-9]{5,20}",
			reason in "[A-Z][a-zA-Z]{5,20}",
		) {
			let exposure = FlagExposure::new(
				&flag_key,
				&variant,
				user_id.clone(),
				&distinct_id,
				&reason,
			);

			// Serialize and deserialize
			let json = serde_json::to_string(&exposure).unwrap();
			let deserialized: FlagExposure = serde_json::from_str(&json).unwrap();

			prop_assert_eq!(&deserialized.flag_key, &flag_key);
			prop_assert_eq!(&deserialized.variant, &variant);
			prop_assert_eq!(&deserialized.user_id, &user_id);
			prop_assert_eq!(&deserialized.distinct_id, &distinct_id);
			prop_assert_eq!(&deserialized.evaluation_reason, &reason);
		}

		#[test]
		fn event_properties_has_required_fields(
			flag_key in "[a-z][a-z0-9._]{0,50}",
			variant in "[a-zA-Z0-9_]{1,20}",
		) {
			let exposure = FlagExposure::new(
				&flag_key,
				&variant,
				None,
				"user",
				"Default",
			);

			let props = exposure.to_event_properties();

			prop_assert!(props.get("$feature_flag").is_some());
			prop_assert!(props.get("$feature_flag_response").is_some());
			prop_assert!(props.get("$feature_flag_reason").is_some());
			prop_assert_eq!(props["$feature_flag"].as_str(), Some(flag_key.as_str()));
			prop_assert_eq!(props["$feature_flag_response"].as_str(), Some(variant.as_str()));
		}
	}
}
