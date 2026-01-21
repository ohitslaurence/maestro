// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{EnvironmentId, OrgId};

/// Deployment environment with its own SDK keys and flag configs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
	pub id: EnvironmentId,
	pub org_id: OrgId,
	/// e.g., "dev", "staging", "prod"
	pub name: String,
	/// For UI display
	pub color: Option<String>,
	pub created_at: DateTime<Utc>,
}

impl Environment {
	/// Default environments to create for new organizations.
	pub const DEFAULT_ENVIRONMENTS: &'static [(&'static str, &'static str)] =
		&[("dev", "#10b981"), ("prod", "#ef4444")];

	/// Returns an iterator over the default environments (name, color).
	pub fn default_environments() -> impl Iterator<Item = (&'static str, &'static str)> {
		Self::DEFAULT_ENVIRONMENTS.iter().copied()
	}

	/// Validates the environment name format.
	///
	/// Valid names:
	/// - Lowercase alphanumeric with underscores
	/// - 2-50 characters
	pub fn validate_name(name: &str) -> bool {
		if name.len() < 2 || name.len() > 50 {
			return false;
		}

		let mut chars = name.chars();

		// First character must be lowercase letter
		match chars.next() {
			Some(c) if c.is_ascii_lowercase() => {}
			_ => return false,
		}

		chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
	}

	/// Validates a color in hex format (e.g., "#10b981").
	pub fn validate_color(color: &str) -> bool {
		if color.len() != 7 {
			return false;
		}

		let mut chars = color.chars();
		if chars.next() != Some('#') {
			return false;
		}

		chars.all(|c| c.is_ascii_hexdigit())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	#[test]
	fn test_validate_name_valid() {
		assert!(Environment::validate_name("dev"));
		assert!(Environment::validate_name("prod"));
		assert!(Environment::validate_name("staging"));
		assert!(Environment::validate_name("qa"));
		assert!(Environment::validate_name("test_env"));
		assert!(Environment::validate_name("prod1"));
	}

	#[test]
	fn test_validate_name_invalid() {
		// Too short
		assert!(!Environment::validate_name("a"));
		assert!(!Environment::validate_name(""));

		// Uppercase
		assert!(!Environment::validate_name("Dev"));
		assert!(!Environment::validate_name("PROD"));

		// Invalid characters
		assert!(!Environment::validate_name("my-env"));
		assert!(!Environment::validate_name("my env"));
		assert!(!Environment::validate_name("my.env"));

		// Starts with number
		assert!(!Environment::validate_name("1env"));

		// Starts with underscore
		assert!(!Environment::validate_name("_env"));
	}

	#[test]
	fn test_validate_color_valid() {
		assert!(Environment::validate_color("#10b981"));
		assert!(Environment::validate_color("#ef4444"));
		assert!(Environment::validate_color("#FFFFFF"));
		assert!(Environment::validate_color("#000000"));
		assert!(Environment::validate_color("#abc123"));
	}

	#[test]
	fn test_validate_color_invalid() {
		// Missing #
		assert!(!Environment::validate_color("10b981"));

		// Too short
		assert!(!Environment::validate_color("#fff"));
		assert!(!Environment::validate_color("#"));

		// Too long
		assert!(!Environment::validate_color("#1234567"));

		// Invalid characters
		assert!(!Environment::validate_color("#gggggg"));
		assert!(!Environment::validate_color("#abc-23"));
	}

	#[test]
	fn test_default_environments() {
		let defaults: Vec<_> = Environment::default_environments().collect();
		assert_eq!(defaults.len(), 2);
		assert!(defaults.contains(&("dev", "#10b981")));
		assert!(defaults.contains(&("prod", "#ef4444")));

		// Verify all default environments have valid names and colors
		for (name, color) in defaults {
			assert!(
				Environment::validate_name(name),
				"Default env '{}' has invalid name",
				name
			);
			assert!(
				Environment::validate_color(color),
				"Default env '{}' has invalid color",
				color
			);
		}
	}

	proptest! {
		/// Valid environment names should always pass validation.
		#[test]
		fn valid_env_names_pass(name in "[a-z][a-z0-9_]{1,49}") {
			prop_assert!(Environment::validate_name(&name));
		}

		/// Names starting with uppercase should fail.
		#[test]
		fn uppercase_start_fails(name in "[A-Z][a-z0-9_]{1,20}") {
			prop_assert!(!Environment::validate_name(&name));
		}

		/// Names starting with numbers should fail.
		#[test]
		fn numeric_start_fails(name in "[0-9][a-z0-9_]{1,20}") {
			prop_assert!(!Environment::validate_name(&name));
		}

		/// Names starting with underscore should fail.
		#[test]
		fn underscore_start_fails(name in "_[a-z0-9_]{1,20}") {
			prop_assert!(!Environment::validate_name(&name));
		}

		/// Valid hex colors should pass validation.
		#[test]
		fn valid_hex_colors_pass(hex in "[0-9a-fA-F]{6}") {
			let color = format!("#{}", hex);
			prop_assert!(Environment::validate_color(&color));
		}

		/// Colors without # prefix should fail.
		#[test]
		fn colors_without_hash_fail(hex in "[0-9a-fA-F]{6}") {
			prop_assert!(!Environment::validate_color(&hex));
		}

		/// Colors with wrong length should fail.
		#[test]
		fn wrong_length_colors_fail(hex in "[0-9a-fA-F]{1,5}") {
			let color = format!("#{}", hex);
			prop_assert!(!Environment::validate_color(&color));
		}
	}
}
