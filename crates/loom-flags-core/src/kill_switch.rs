// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{OrgId, UserId};

/// Unique identifier for a kill switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KillSwitchId(pub Uuid);

impl KillSwitchId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for KillSwitchId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for KillSwitchId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for KillSwitchId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// Emergency shutoff mechanism that overrides linked flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillSwitch {
	pub id: KillSwitchId,
	/// None = platform-level
	pub org_id: Option<OrgId>,
	/// e.g., "disable_checkout"
	pub key: String,
	pub name: String,
	pub description: Option<String>,
	pub linked_flag_keys: Vec<String>,
	pub is_active: bool,
	pub activated_at: Option<DateTime<Utc>>,
	pub activated_by: Option<UserId>,
	pub activation_reason: Option<String>,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

impl KillSwitch {
	/// Validates the kill switch key format.
	///
	/// Uses the same rules as flag keys:
	/// - Lowercase alphanumeric with underscores
	/// - 3-100 characters
	/// - Pattern: `^[a-z][a-z0-9_]*$`
	pub fn validate_key(key: &str) -> bool {
		if key.len() < 3 || key.len() > 100 {
			return false;
		}

		let mut chars = key.chars();

		// First character must be lowercase letter
		match chars.next() {
			Some(c) if c.is_ascii_lowercase() => {}
			_ => return false,
		}

		chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
	}

	/// Checks if this kill switch affects a given flag key.
	pub fn affects_flag(&self, flag_key: &str) -> bool {
		self.is_active && self.linked_flag_keys.iter().any(|k| k == flag_key)
	}

	/// Activates the kill switch.
	pub fn activate(&mut self, activated_by: UserId, reason: String) {
		self.is_active = true;
		self.activated_at = Some(Utc::now());
		self.activated_by = Some(activated_by);
		self.activation_reason = Some(reason);
		self.updated_at = Utc::now();
	}

	/// Deactivates the kill switch.
	pub fn deactivate(&mut self) {
		self.is_active = false;
		self.activated_at = None;
		self.activated_by = None;
		self.activation_reason = None;
		self.updated_at = Utc::now();
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_validate_key_valid() {
		assert!(KillSwitch::validate_key("disable_checkout"));
		assert!(KillSwitch::validate_key("emergency_stop"));
		assert!(KillSwitch::validate_key("abc"));
		assert!(KillSwitch::validate_key("a_b_c"));
		assert!(KillSwitch::validate_key("a1b2c3"));
	}

	#[test]
	fn test_validate_key_invalid() {
		// Too short
		assert!(!KillSwitch::validate_key("ab"));
		assert!(!KillSwitch::validate_key("a"));
		assert!(!KillSwitch::validate_key(""));

		// Uppercase
		assert!(!KillSwitch::validate_key("Disable"));
		assert!(!KillSwitch::validate_key("DISABLE"));

		// Invalid characters
		assert!(!KillSwitch::validate_key("disable-checkout"));
		assert!(!KillSwitch::validate_key("disable checkout"));
		assert!(!KillSwitch::validate_key("disable.checkout"));

		// Starts with number
		assert!(!KillSwitch::validate_key("1disable"));

		// Starts with underscore
		assert!(!KillSwitch::validate_key("_disable"));
	}

	#[test]
	fn test_affects_flag() {
		let kill_switch = KillSwitch {
			id: KillSwitchId::new(),
			org_id: None,
			key: "disable_checkout".to_string(),
			name: "Disable Checkout".to_string(),
			description: None,
			linked_flag_keys: vec![
				"checkout.new_flow".to_string(),
				"checkout.payment".to_string(),
			],
			is_active: true,
			activated_at: Some(Utc::now()),
			activated_by: Some(UserId::new()),
			activation_reason: Some("Emergency".to_string()),
			created_at: Utc::now(),
			updated_at: Utc::now(),
		};

		assert!(kill_switch.affects_flag("checkout.new_flow"));
		assert!(kill_switch.affects_flag("checkout.payment"));
		assert!(!kill_switch.affects_flag("billing.subscription"));
	}

	#[test]
	fn test_affects_flag_inactive() {
		let kill_switch = KillSwitch {
			id: KillSwitchId::new(),
			org_id: None,
			key: "disable_checkout".to_string(),
			name: "Disable Checkout".to_string(),
			description: None,
			linked_flag_keys: vec!["checkout.new_flow".to_string()],
			is_active: false,
			activated_at: None,
			activated_by: None,
			activation_reason: None,
			created_at: Utc::now(),
			updated_at: Utc::now(),
		};

		// Inactive kill switch doesn't affect flags
		assert!(!kill_switch.affects_flag("checkout.new_flow"));
	}

	#[test]
	fn test_activate_deactivate() {
		let mut kill_switch = KillSwitch {
			id: KillSwitchId::new(),
			org_id: None,
			key: "disable_checkout".to_string(),
			name: "Disable Checkout".to_string(),
			description: None,
			linked_flag_keys: vec!["checkout.new_flow".to_string()],
			is_active: false,
			activated_at: None,
			activated_by: None,
			activation_reason: None,
			created_at: Utc::now(),
			updated_at: Utc::now(),
		};

		let user_id = UserId::new();
		kill_switch.activate(user_id, "Testing".to_string());

		assert!(kill_switch.is_active);
		assert!(kill_switch.activated_at.is_some());
		assert_eq!(kill_switch.activated_by, Some(user_id));
		assert_eq!(kill_switch.activation_reason, Some("Testing".to_string()));

		kill_switch.deactivate();

		assert!(!kill_switch.is_active);
		assert!(kill_switch.activated_at.is_none());
		assert!(kill_switch.activated_by.is_none());
		assert!(kill_switch.activation_reason.is_none());
	}
}
