// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::StrategyId;

/// Unique identifier for a feature flag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlagId(pub Uuid);

impl FlagId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for FlagId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for FlagId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for FlagId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// Unique identifier for a flag config (per-environment).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FlagConfigId(pub Uuid);

impl FlagConfigId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for FlagConfigId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for FlagConfigId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for FlagConfigId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// Unique identifier for an organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrgId(pub Uuid);

impl OrgId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for OrgId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for OrgId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for OrgId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// Unique identifier for a user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

impl UserId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for UserId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for UserId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for UserId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// A feature flag with multi-variant support and per-environment configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flag {
	pub id: FlagId,
	/// None = platform-level flag
	pub org_id: Option<OrgId>,
	/// Structured key: "checkout.new_flow"
	pub key: String,
	/// Human-readable name
	pub name: String,
	pub description: Option<String>,
	pub tags: Vec<String>,
	pub maintainer_user_id: Option<UserId>,
	pub variants: Vec<Variant>,
	/// Variant name for fallback
	pub default_variant: String,
	pub prerequisites: Vec<FlagPrerequisite>,
	/// Whether to log exposures for this flag (for experiment tracking)
	#[serde(default)]
	pub exposure_tracking_enabled: bool,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
	pub archived_at: Option<DateTime<Utc>>,
}

impl Flag {
	/// Validates the flag key format.
	///
	/// Valid keys:
	/// - Lowercase alphanumeric with dots and underscores
	/// - 3-100 characters
	/// - Cannot start or end with dot
	/// - Pattern: `^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$`
	pub fn validate_key(key: &str) -> bool {
		if key.len() < 3 || key.len() > 100 {
			return false;
		}

		if key.starts_with('.') || key.ends_with('.') {
			return false;
		}

		let mut chars = key.chars().peekable();

		// First character must be lowercase letter
		match chars.next() {
			Some(c) if c.is_ascii_lowercase() => {}
			_ => return false,
		}

		let mut prev_was_dot = false;
		for c in chars {
			if c == '.' {
				if prev_was_dot {
					return false; // No consecutive dots
				}
				prev_was_dot = true;
			} else if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
				prev_was_dot = false;
			} else {
				return false;
			}
		}

		!prev_was_dot // Cannot end with dot (double-check)
	}

	/// Checks if this flag is archived.
	pub fn is_archived(&self) -> bool {
		self.archived_at.is_some()
	}

	/// Gets a variant by name.
	pub fn get_variant(&self, name: &str) -> Option<&Variant> {
		self.variants.iter().find(|v| v.name == name)
	}

	/// Gets the default variant.
	pub fn get_default_variant(&self) -> Option<&Variant> {
		self.get_variant(&self.default_variant)
	}
}

/// A variant of a feature flag.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Variant {
	/// e.g., "control", "treatment_a"
	pub name: String,
	pub value: VariantValue,
	/// For percentage-based distribution
	pub weight: u32,
}

/// The value of a variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum VariantValue {
	Boolean(bool),
	String(String),
	Json(serde_json::Value),
}

impl VariantValue {
	/// Returns the value as a boolean if it is one.
	pub fn as_bool(&self) -> Option<bool> {
		match self {
			VariantValue::Boolean(b) => Some(*b),
			_ => None,
		}
	}

	/// Returns the value as a string if it is one.
	pub fn as_str(&self) -> Option<&str> {
		match self {
			VariantValue::String(s) => Some(s),
			_ => None,
		}
	}

	/// Returns the value as JSON.
	pub fn as_json(&self) -> Option<&serde_json::Value> {
		match self {
			VariantValue::Json(v) => Some(v),
			_ => None,
		}
	}
}

/// A prerequisite that must be met for a flag to be evaluated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlagPrerequisite {
	pub flag_key: String,
	/// Prerequisite flag must be this variant
	pub required_variant: String,
}

/// Per-environment configuration for a flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlagConfig {
	pub id: FlagConfigId,
	pub flag_id: FlagId,
	pub environment_id: EnvironmentId,
	pub enabled: bool,
	pub strategy_id: Option<StrategyId>,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

/// Unique identifier for an environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EnvironmentId(pub Uuid);

impl EnvironmentId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for EnvironmentId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for EnvironmentId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for EnvironmentId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	#[test]
	fn test_validate_flag_key_valid() {
		assert!(Flag::validate_key("checkout"));
		assert!(Flag::validate_key("checkout.new_flow"));
		assert!(Flag::validate_key("billing.subscription.annual"));
		assert!(Flag::validate_key("ai.model.gpt4"));
		assert!(Flag::validate_key("abc"));
		assert!(Flag::validate_key("a_b"));
		assert!(Flag::validate_key("a1b"));
	}

	#[test]
	fn test_validate_flag_key_invalid() {
		// Too short
		assert!(!Flag::validate_key("ab"));
		assert!(!Flag::validate_key("a"));
		assert!(!Flag::validate_key(""));

		// Starts with dot
		assert!(!Flag::validate_key(".checkout"));

		// Ends with dot
		assert!(!Flag::validate_key("checkout."));

		// Consecutive dots
		assert!(!Flag::validate_key("checkout..flow"));

		// Uppercase
		assert!(!Flag::validate_key("Checkout"));
		assert!(!Flag::validate_key("checkOut"));

		// Invalid characters
		assert!(!Flag::validate_key("check-out"));
		assert!(!Flag::validate_key("check out"));
		assert!(!Flag::validate_key("checkout!"));

		// Starts with number
		assert!(!Flag::validate_key("1checkout"));

		// Starts with underscore
		assert!(!Flag::validate_key("_checkout"));
	}

	#[test]
	fn test_variant_value_accessors() {
		let bool_val = VariantValue::Boolean(true);
		assert_eq!(bool_val.as_bool(), Some(true));
		assert_eq!(bool_val.as_str(), None);
		assert_eq!(bool_val.as_json(), None);

		let str_val = VariantValue::String("test".to_string());
		assert_eq!(str_val.as_bool(), None);
		assert_eq!(str_val.as_str(), Some("test"));
		assert_eq!(str_val.as_json(), None);

		let json_val = VariantValue::Json(serde_json::json!({"key": "value"}));
		assert_eq!(json_val.as_bool(), None);
		assert_eq!(json_val.as_str(), None);
		assert!(json_val.as_json().is_some());
	}

	// Property-based test strategies

	fn valid_segment() -> impl Strategy<Value = String> {
		prop::collection::vec(
			prop_oneof![
				prop::char::range('a', 'z'),
				prop::char::range('0', '9'),
				Just('_')
			],
			1..10,
		)
		.prop_filter_map("must start with letter", |chars| {
			if chars
				.first()
				.map(|c| c.is_ascii_lowercase())
				.unwrap_or(false)
			{
				Some(chars.into_iter().collect())
			} else {
				None
			}
		})
	}

	fn valid_flag_key() -> impl Strategy<Value = String> {
		prop::collection::vec(valid_segment(), 1..5).prop_filter_map(
			"must be 3-100 chars",
			|segments| {
				let key = segments.join(".");
				if key.len() >= 3 && key.len() <= 100 {
					Some(key)
				} else {
					None
				}
			},
		)
	}

	proptest! {
		#[test]
		fn prop_valid_keys_pass_validation(key in valid_flag_key()) {
			prop_assert!(Flag::validate_key(&key), "Key '{}' should be valid", key);
		}

		#[test]
		fn prop_keys_with_uppercase_fail(
			base in valid_flag_key(),
			idx in 0usize..100
		) {
			if !base.is_empty() {
				let idx = idx % base.len();
				let mut chars: Vec<char> = base.chars().collect();
				if chars[idx].is_ascii_lowercase() {
					chars[idx] = chars[idx].to_ascii_uppercase();
					let invalid_key: String = chars.into_iter().collect();
					prop_assert!(!Flag::validate_key(&invalid_key),
						"Key '{}' with uppercase should be invalid", invalid_key);
				}
			}
		}

		#[test]
		fn prop_short_keys_fail(key in "[a-z][a-z0-9_]{0,1}") {
			// Keys with 1-2 chars should fail
			if key.len() < 3 {
				prop_assert!(!Flag::validate_key(&key),
					"Short key '{}' should be invalid", key);
			}
		}

		#[test]
		fn prop_keys_starting_with_dot_fail(key in "\\.[a-z][a-z0-9_.]{0,50}") {
			prop_assert!(!Flag::validate_key(&key),
				"Key '{}' starting with dot should be invalid", key);
		}

		#[test]
		fn prop_keys_ending_with_dot_fail(key in "[a-z][a-z0-9_.]{0,50}\\.") {
			prop_assert!(!Flag::validate_key(&key),
				"Key '{}' ending with dot should be invalid", key);
		}

		#[test]
		fn prop_keys_with_consecutive_dots_fail(key in "[a-z][a-z0-9_]{0,20}\\.\\.[a-z0-9_]{0,20}") {
			prop_assert!(!Flag::validate_key(&key),
				"Key '{}' with consecutive dots should be invalid", key);
		}

		#[test]
		fn prop_keys_starting_with_number_fail(key in "[0-9][a-z0-9_.]{2,50}") {
			prop_assert!(!Flag::validate_key(&key),
				"Key '{}' starting with number should be invalid", key);
		}

		#[test]
		fn prop_keys_starting_with_underscore_fail(key in "_[a-z0-9_.]{2,50}") {
			prop_assert!(!Flag::validate_key(&key),
				"Key '{}' starting with underscore should be invalid", key);
		}

		#[test]
		fn prop_keys_with_invalid_chars_fail(key in "[a-z][a-z0-9_]*[-!@#$%^&*()][a-z0-9_]*") {
			prop_assert!(!Flag::validate_key(&key),
				"Key '{}' with invalid chars should be invalid", key);
		}
	}
}
