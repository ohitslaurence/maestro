// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{KillSwitchId, StrategyId, VariantValue};

/// Context passed by SDK for flag evaluation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvaluationContext {
	pub user_id: Option<String>,
	pub org_id: Option<String>,
	pub session_id: Option<String>,
	pub environment: String,
	pub attributes: HashMap<String, serde_json::Value>,
	/// GeoIP resolved server-side from request IP
	#[serde(default)]
	pub geo: Option<GeoContext>,
}

impl EvaluationContext {
	pub fn new(environment: impl Into<String>) -> Self {
		Self {
			environment: environment.into(),
			..Default::default()
		}
	}

	pub fn with_user_id(mut self, user_id: impl Into<String>) -> Self {
		self.user_id = Some(user_id.into());
		self
	}

	pub fn with_org_id(mut self, org_id: impl Into<String>) -> Self {
		self.org_id = Some(org_id.into());
		self
	}

	pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
		self.session_id = Some(session_id.into());
		self
	}

	pub fn with_attribute(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
		self.attributes.insert(key.into(), value);
		self
	}

	pub fn with_geo(mut self, geo: GeoContext) -> Self {
		self.geo = Some(geo);
		self
	}

	/// Computes a deterministic hash of the evaluation context for deduplication.
	///
	/// The hash is based on:
	/// - flag_key (to scope dedup per flag)
	/// - user_id
	/// - org_id
	/// - session_id
	/// - environment
	/// - all attributes (sorted by key for determinism)
	/// - geo context
	///
	/// Returns a hex-encoded SHA-256 hash.
	pub fn compute_hash(&self, flag_key: &str) -> String {
		use sha2::{Digest, Sha256};

		let mut hasher = Sha256::new();

		// Include flag key
		hasher.update(flag_key.as_bytes());
		hasher.update(b"|");

		// Include user_id
		if let Some(ref user_id) = self.user_id {
			hasher.update(user_id.as_bytes());
		}
		hasher.update(b"|");

		// Include org_id
		if let Some(ref org_id) = self.org_id {
			hasher.update(org_id.as_bytes());
		}
		hasher.update(b"|");

		// Include session_id
		if let Some(ref session_id) = self.session_id {
			hasher.update(session_id.as_bytes());
		}
		hasher.update(b"|");

		// Include environment
		hasher.update(self.environment.as_bytes());
		hasher.update(b"|");

		// Include attributes (sorted for determinism)
		let mut keys: Vec<_> = self.attributes.keys().collect();
		keys.sort();
		for key in keys {
			hasher.update(key.as_bytes());
			hasher.update(b"=");
			if let Some(value) = self.attributes.get(key) {
				hasher.update(value.to_string().as_bytes());
			}
			hasher.update(b",");
		}
		hasher.update(b"|");

		// Include geo context
		if let Some(ref geo) = self.geo {
			if let Some(ref country) = geo.country {
				hasher.update(country.as_bytes());
			}
			hasher.update(b"/");
			if let Some(ref region) = geo.region {
				hasher.update(region.as_bytes());
			}
			hasher.update(b"/");
			if let Some(ref city) = geo.city {
				hasher.update(city.as_bytes());
			}
		}

		// Return hex-encoded hash
		hex::encode(hasher.finalize())
	}
}

/// GeoIP context resolved from client IP.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeoContext {
	pub country: Option<String>,
	pub region: Option<String>,
	pub city: Option<String>,
}

impl GeoContext {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn with_country(mut self, country: impl Into<String>) -> Self {
		self.country = Some(country.into());
		self
	}

	pub fn with_region(mut self, region: impl Into<String>) -> Self {
		self.region = Some(region.into());
		self
	}

	pub fn with_city(mut self, city: impl Into<String>) -> Self {
		self.city = Some(city.into());
		self
	}
}

/// Result of evaluating a feature flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvaluationResult {
	pub flag_key: String,
	pub variant: String,
	pub value: VariantValue,
	pub reason: EvaluationReason,
}

impl EvaluationResult {
	/// Creates a new evaluation result.
	pub fn new(
		flag_key: impl Into<String>,
		variant: impl Into<String>,
		value: VariantValue,
		reason: EvaluationReason,
	) -> Self {
		Self {
			flag_key: flag_key.into(),
			variant: variant.into(),
			value,
			reason,
		}
	}

	/// Creates an evaluation result for when a flag is not found.
	pub fn not_found(flag_key: impl Into<String>) -> Self {
		Self {
			flag_key: flag_key.into(),
			variant: String::new(),
			value: VariantValue::Boolean(false),
			reason: EvaluationReason::Error {
				message: "Flag not found".to_string(),
			},
		}
	}
}

/// The reason for an evaluation result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum EvaluationReason {
	/// No strategy, default variant used
	Default,
	/// Strategy determined the variant
	Strategy { strategy_id: StrategyId },
	/// Kill switch forced the flag off
	KillSwitch { kill_switch_id: KillSwitchId },
	/// Prerequisite flag not met
	Prerequisite { missing_flag: String },
	/// Flag disabled in this environment
	Disabled,
	/// An error occurred during evaluation
	Error { message: String },
}

impl EvaluationReason {
	pub fn is_error(&self) -> bool {
		matches!(self, EvaluationReason::Error { .. })
	}
}

/// Bulk evaluation results for all flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkEvaluationResult {
	pub results: Vec<EvaluationResult>,
	pub evaluated_at: chrono::DateTime<chrono::Utc>,
}

impl BulkEvaluationResult {
	pub fn new(results: Vec<EvaluationResult>) -> Self {
		Self {
			results,
			evaluated_at: chrono::Utc::now(),
		}
	}

	/// Gets the result for a specific flag.
	pub fn get(&self, flag_key: &str) -> Option<&EvaluationResult> {
		self.results.iter().find(|r| r.flag_key == flag_key)
	}

	/// Gets the boolean value for a flag, returning the default if not found or not a boolean.
	pub fn get_bool(&self, flag_key: &str, default: bool) -> bool {
		self
			.get(flag_key)
			.and_then(|r| r.value.as_bool())
			.unwrap_or(default)
	}

	/// Gets the string value for a flag, returning the default if not found or not a string.
	pub fn get_string<'a>(&'a self, flag_key: &str, default: &'a str) -> &'a str {
		self
			.get(flag_key)
			.and_then(|r| r.value.as_str())
			.unwrap_or(default)
	}
}

/// Statistics for a flag's usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagStats {
	pub flag_key: String,
	pub last_evaluated_at: Option<chrono::DateTime<chrono::Utc>>,
	pub evaluation_count_24h: u64,
	pub evaluation_count_7d: u64,
	pub evaluation_count_30d: u64,
}

/// Exposure log entry for experiment tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExposureLog {
	pub id: ExposureLogId,
	pub flag_id: crate::FlagId,
	pub flag_key: String,
	pub environment_id: crate::EnvironmentId,
	pub user_id: Option<String>,
	pub org_id: Option<String>,
	pub variant: String,
	pub reason: EvaluationReason,
	/// Hash of evaluation context for dedup
	pub context_hash: String,
	pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ExposureLog {
	/// Creates a new exposure log entry.
	#[allow(clippy::too_many_arguments)]
	pub fn new(
		flag_id: crate::FlagId,
		flag_key: impl Into<String>,
		environment_id: crate::EnvironmentId,
		user_id: Option<String>,
		org_id: Option<String>,
		variant: impl Into<String>,
		reason: EvaluationReason,
		context_hash: impl Into<String>,
	) -> Self {
		Self {
			id: ExposureLogId::new(),
			flag_id,
			flag_key: flag_key.into(),
			environment_id,
			user_id,
			org_id,
			variant: variant.into(),
			reason,
			context_hash: context_hash.into(),
			timestamp: chrono::Utc::now(),
		}
	}
}

/// Unique identifier for an exposure log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ExposureLogId(pub uuid::Uuid);

impl ExposureLogId {
	pub fn new() -> Self {
		Self(uuid::Uuid::new_v4())
	}
}

impl Default for ExposureLogId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for ExposureLogId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_evaluation_context_builder() {
		let ctx = EvaluationContext::new("prod")
			.with_user_id("user123")
			.with_org_id("org456")
			.with_session_id("sess789")
			.with_attribute("plan", serde_json::json!("enterprise"))
			.with_geo(GeoContext::new().with_country("US").with_region("CA"));

		assert_eq!(ctx.environment, "prod");
		assert_eq!(ctx.user_id, Some("user123".to_string()));
		assert_eq!(ctx.org_id, Some("org456".to_string()));
		assert_eq!(ctx.session_id, Some("sess789".to_string()));
		assert_eq!(
			ctx.attributes.get("plan"),
			Some(&serde_json::json!("enterprise"))
		);
		assert!(ctx.geo.is_some());
		assert_eq!(ctx.geo.as_ref().unwrap().country, Some("US".to_string()));
	}

	#[test]
	fn test_bulk_evaluation_result() {
		let results = vec![
			EvaluationResult::new(
				"feature.enabled",
				"on",
				VariantValue::Boolean(true),
				EvaluationReason::Default,
			),
			EvaluationResult::new(
				"feature.theme",
				"dark",
				VariantValue::String("dark".to_string()),
				EvaluationReason::Default,
			),
		];

		let bulk = BulkEvaluationResult::new(results);

		assert!(bulk.get_bool("feature.enabled", false));
		assert!(!bulk.get_bool("feature.nonexistent", false));
		assert_eq!(bulk.get_string("feature.theme", "light"), "dark");
		assert_eq!(bulk.get_string("feature.nonexistent", "light"), "light");
	}

	#[test]
	fn test_evaluation_reason_is_error() {
		assert!(!EvaluationReason::Default.is_error());
		assert!(!EvaluationReason::Disabled.is_error());
		assert!(EvaluationReason::Error {
			message: "test".to_string()
		}
		.is_error());
	}

	#[test]
	fn test_context_hash_deterministic() {
		let ctx = EvaluationContext::new("prod")
			.with_user_id("user123")
			.with_org_id("org456")
			.with_attribute("plan", serde_json::json!("enterprise"));

		let hash1 = ctx.compute_hash("feature.test");
		let hash2 = ctx.compute_hash("feature.test");

		assert_eq!(hash1, hash2);
	}

	#[test]
	fn test_context_hash_different_for_different_flags() {
		let ctx = EvaluationContext::new("prod").with_user_id("user123");

		let hash1 = ctx.compute_hash("feature.flag_a");
		let hash2 = ctx.compute_hash("feature.flag_b");

		assert_ne!(hash1, hash2);
	}

	#[test]
	fn test_context_hash_different_for_different_users() {
		let ctx1 = EvaluationContext::new("prod").with_user_id("user123");
		let ctx2 = EvaluationContext::new("prod").with_user_id("user456");

		let hash1 = ctx1.compute_hash("feature.test");
		let hash2 = ctx2.compute_hash("feature.test");

		assert_ne!(hash1, hash2);
	}

	#[test]
	fn test_context_hash_is_hex_sha256() {
		let ctx = EvaluationContext::new("prod");
		let hash = ctx.compute_hash("test");

		// SHA-256 produces 64 hex characters
		assert_eq!(hash.len(), 64);
		// All characters should be valid hex
		assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
	}

	#[test]
	fn test_context_hash_includes_attributes() {
		let ctx1 = EvaluationContext::new("prod").with_attribute("plan", serde_json::json!("free"));
		let ctx2 =
			EvaluationContext::new("prod").with_attribute("plan", serde_json::json!("enterprise"));

		let hash1 = ctx1.compute_hash("feature.test");
		let hash2 = ctx2.compute_hash("feature.test");

		assert_ne!(hash1, hash2);
	}

	#[test]
	fn test_context_hash_includes_geo() {
		let ctx1 = EvaluationContext::new("prod").with_geo(GeoContext::new().with_country("US"));
		let ctx2 = EvaluationContext::new("prod").with_geo(GeoContext::new().with_country("GB"));

		let hash1 = ctx1.compute_hash("feature.test");
		let hash2 = ctx2.compute_hash("feature.test");

		assert_ne!(hash1, hash2);
	}

	#[test]
	fn test_context_hash_attributes_order_independent() {
		// Hash should be the same regardless of attribute insertion order
		let ctx1 = EvaluationContext::new("prod")
			.with_attribute("a", serde_json::json!("1"))
			.with_attribute("b", serde_json::json!("2"))
			.with_attribute("c", serde_json::json!("3"));

		let ctx2 = EvaluationContext::new("prod")
			.with_attribute("c", serde_json::json!("3"))
			.with_attribute("a", serde_json::json!("1"))
			.with_attribute("b", serde_json::json!("2"));

		let hash1 = ctx1.compute_hash("feature.test");
		let hash2 = ctx2.compute_hash("feature.test");

		assert_eq!(hash1, hash2);
	}

	#[test]
	fn test_flag_stats_creation() {
		let stats = FlagStats {
			flag_key: "feature.test".to_string(),
			last_evaluated_at: Some(chrono::Utc::now()),
			evaluation_count_24h: 100,
			evaluation_count_7d: 500,
			evaluation_count_30d: 2000,
		};

		assert_eq!(stats.flag_key, "feature.test");
		assert!(stats.last_evaluated_at.is_some());
		assert_eq!(stats.evaluation_count_24h, 100);
		assert_eq!(stats.evaluation_count_7d, 500);
		assert_eq!(stats.evaluation_count_30d, 2000);
	}

	#[test]
	fn test_flag_stats_never_evaluated() {
		let stats = FlagStats {
			flag_key: "feature.new".to_string(),
			last_evaluated_at: None,
			evaluation_count_24h: 0,
			evaluation_count_7d: 0,
			evaluation_count_30d: 0,
		};

		assert!(stats.last_evaluated_at.is_none());
		assert_eq!(stats.evaluation_count_24h, 0);
	}

	#[test]
	fn test_flag_stats_serialization() {
		let stats = FlagStats {
			flag_key: "feature.test".to_string(),
			last_evaluated_at: None,
			evaluation_count_24h: 42,
			evaluation_count_7d: 300,
			evaluation_count_30d: 1000,
		};

		let json = serde_json::to_string(&stats).unwrap();
		let deserialized: FlagStats = serde_json::from_str(&json).unwrap();

		assert_eq!(deserialized.flag_key, stats.flag_key);
		assert_eq!(
			deserialized.evaluation_count_24h,
			stats.evaluation_count_24h
		);
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		/// Property: Evaluation counts should maintain invariant 24h <= 7d <= 30d
		/// This tests that our FlagStats can handle any valid count values
		#[test]
		fn flag_stats_count_invariants(
			count_24h in 0u64..1_000_000,
			count_7d_extra in 0u64..10_000_000,
			count_30d_extra in 0u64..100_000_000,
		) {
			// Build counts that maintain the invariant
			let count_7d = count_24h.saturating_add(count_7d_extra);
			let count_30d = count_7d.saturating_add(count_30d_extra);

			let stats = FlagStats {
				flag_key: "test.flag".to_string(),
				last_evaluated_at: None,
				evaluation_count_24h: count_24h,
				evaluation_count_7d: count_7d,
				evaluation_count_30d: count_30d,
			};

			// Verify invariants
			prop_assert!(stats.evaluation_count_24h <= stats.evaluation_count_7d);
			prop_assert!(stats.evaluation_count_7d <= stats.evaluation_count_30d);
		}

		/// Property: Context hash is deterministic
		#[test]
		fn context_hash_is_deterministic(
			environment in "[a-z]+",
			user_id in prop::option::of("[a-z0-9]+"),
			flag_key in "[a-z.]+",
		) {
			let mut ctx = EvaluationContext::new(&environment);
			if let Some(uid) = &user_id {
				ctx = ctx.with_user_id(uid);
			}

			let hash1 = ctx.compute_hash(&flag_key);
			let hash2 = ctx.compute_hash(&flag_key);

			prop_assert_eq!(hash1, hash2);
		}

		/// Property: Different contexts produce different hashes
		#[test]
		fn different_users_produce_different_hashes(
			user_id1 in "[a-z0-9]{5,10}",
			user_id2 in "[a-z0-9]{5,10}",
			flag_key in "[a-z.]+",
		) {
			prop_assume!(user_id1 != user_id2);

			let ctx1 = EvaluationContext::new("prod").with_user_id(&user_id1);
			let ctx2 = EvaluationContext::new("prod").with_user_id(&user_id2);

			let hash1 = ctx1.compute_hash(&flag_key);
			let hash2 = ctx2.compute_hash(&flag_key);

			prop_assert_ne!(hash1, hash2);
		}

		/// Property: Hash is always 64 hex characters (SHA-256)
		#[test]
		fn hash_is_always_valid_sha256(
			environment in "[a-z]+",
			user_id in prop::option::of("[a-z0-9]+"),
			flag_key in "[a-z.]+",
		) {
			let mut ctx = EvaluationContext::new(&environment);
			if let Some(uid) = &user_id {
				ctx = ctx.with_user_id(uid);
			}

			let hash = ctx.compute_hash(&flag_key);

			prop_assert_eq!(hash.len(), 64);
			prop_assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
		}
	}
}
