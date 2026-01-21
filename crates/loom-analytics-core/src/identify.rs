// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Payload types for identity resolution operations.
//!
//! These types are used by the identify, alias, and property-setting APIs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::person::PersonId;

/// Payload for the identify operation, linking an anonymous ID to a user ID.
///
/// When a user logs in, call identify to link their anonymous session
/// (distinct_id) to their authenticated user_id. This enables tracking
/// the user's journey before and after authentication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentifyPayload {
	pub distinct_id: String,
	pub user_id: String,
	#[serde(default)]
	pub properties: serde_json::Value,
}

impl IdentifyPayload {
	/// Creates a new identify payload.
	pub fn new(distinct_id: String, user_id: String) -> Self {
		Self {
			distinct_id,
			user_id,
			properties: serde_json::json!({}),
		}
	}

	/// Sets properties to update on the person (builder pattern).
	pub fn with_properties(mut self, properties: serde_json::Value) -> Self {
		self.properties = properties;
		self
	}
}

/// Payload for the alias operation, linking two distinct IDs.
///
/// Alias creates a link between two distinct IDs, merging their persons
/// if they were previously separate. Use this when you have multiple
/// identifiers for the same user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AliasPayload {
	pub distinct_id: String,
	pub alias: String,
}

impl AliasPayload {
	/// Creates a new alias payload.
	pub fn new(distinct_id: String, alias: String) -> Self {
		Self { distinct_id, alias }
	}
}

/// Payload for setting person properties (overwrites existing values).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetPayload {
	pub distinct_id: String,
	#[serde(default)]
	pub properties: serde_json::Value,
}

impl SetPayload {
	/// Creates a new set payload.
	pub fn new(distinct_id: String, properties: serde_json::Value) -> Self {
		Self {
			distinct_id,
			properties,
		}
	}
}

/// Payload for setting properties only if they don't already exist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetOncePayload {
	pub distinct_id: String,
	#[serde(default)]
	pub properties: serde_json::Value,
}

impl SetOncePayload {
	/// Creates a new set-once payload.
	pub fn new(distinct_id: String, properties: serde_json::Value) -> Self {
		Self {
			distinct_id,
			properties,
		}
	}
}

/// Payload for removing properties from a person.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnsetPayload {
	pub distinct_id: String,
	pub properties: Vec<String>,
}

impl UnsetPayload {
	/// Creates a new unset payload.
	pub fn new(distinct_id: String, properties: Vec<String>) -> Self {
		Self {
			distinct_id,
			properties,
		}
	}
}

/// Unique identifier for a person merge record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PersonMergeId(pub Uuid);

impl PersonMergeId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for PersonMergeId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for PersonMergeId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for PersonMergeId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// An audit record of two persons being merged.
///
/// When identity resolution determines two persons represent the same user,
/// the "loser" is merged into the "winner". This record provides an audit
/// trail of the merge operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonMerge {
	pub id: PersonMergeId,
	pub winner_id: PersonId,
	pub loser_id: PersonId,
	pub reason: MergeReason,
	pub merged_at: DateTime<Utc>,
}

impl PersonMerge {
	/// Creates a new merge record.
	pub fn new(winner_id: PersonId, loser_id: PersonId, reason: MergeReason) -> Self {
		Self {
			id: PersonMergeId::new(),
			winner_id,
			loser_id,
			reason,
			merged_at: Utc::now(),
		}
	}
}

/// The reason why two persons were merged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MergeReason {
	/// Merge triggered by an identify call.
	Identify {
		distinct_id: String,
		user_id: String,
	},
	/// Merge triggered by an alias call.
	Alias { distinct_id: String, alias: String },
	/// Merge triggered manually by an admin.
	Manual { by_user_id: String },
}

impl MergeReason {
	/// Creates an identify merge reason.
	pub fn identify(distinct_id: String, user_id: String) -> Self {
		MergeReason::Identify {
			distinct_id,
			user_id,
		}
	}

	/// Creates an alias merge reason.
	pub fn alias(distinct_id: String, alias: String) -> Self {
		MergeReason::Alias { distinct_id, alias }
	}

	/// Creates a manual merge reason.
	pub fn manual(by_user_id: String) -> Self {
		MergeReason::Manual { by_user_id }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	#[test]
	fn identify_payload_new() {
		let payload = IdentifyPayload::new("anon_123".to_string(), "user@example.com".to_string());
		assert_eq!(payload.distinct_id, "anon_123");
		assert_eq!(payload.user_id, "user@example.com");
		assert_eq!(payload.properties, serde_json::json!({}));
	}

	#[test]
	fn identify_payload_with_properties() {
		let payload = IdentifyPayload::new("anon_123".to_string(), "user@example.com".to_string())
			.with_properties(serde_json::json!({"plan": "pro"}));
		assert_eq!(payload.properties["plan"], "pro");
	}

	#[test]
	fn alias_payload_new() {
		let payload = AliasPayload::new("primary_id".to_string(), "alias_id".to_string());
		assert_eq!(payload.distinct_id, "primary_id");
		assert_eq!(payload.alias, "alias_id");
	}

	#[test]
	fn set_payload_new() {
		let payload = SetPayload::new("user_123".to_string(), serde_json::json!({"name": "Alice"}));
		assert_eq!(payload.distinct_id, "user_123");
		assert_eq!(payload.properties["name"], "Alice");
	}

	#[test]
	fn unset_payload_new() {
		let payload = UnsetPayload::new(
			"user_123".to_string(),
			vec!["name".to_string(), "email".to_string()],
		);
		assert_eq!(payload.distinct_id, "user_123");
		assert_eq!(payload.properties, vec!["name", "email"]);
	}

	#[test]
	fn person_merge_new() {
		let winner_id = PersonId::new();
		let loser_id = PersonId::new();
		let reason = MergeReason::identify("anon_123".to_string(), "user@example.com".to_string());
		let merge = PersonMerge::new(winner_id, loser_id, reason);

		assert_eq!(merge.winner_id, winner_id);
		assert_eq!(merge.loser_id, loser_id);
	}

	#[test]
	fn merge_reason_serde_identify() {
		let reason = MergeReason::identify("anon_123".to_string(), "user@example.com".to_string());
		let json = serde_json::to_string(&reason).unwrap();
		let parsed: MergeReason = serde_json::from_str(&json).unwrap();
		assert_eq!(reason, parsed);
	}

	#[test]
	fn merge_reason_serde_alias() {
		let reason = MergeReason::alias("primary".to_string(), "secondary".to_string());
		let json = serde_json::to_string(&reason).unwrap();
		let parsed: MergeReason = serde_json::from_str(&json).unwrap();
		assert_eq!(reason, parsed);
	}

	#[test]
	fn merge_reason_serde_manual() {
		let reason = MergeReason::manual("admin_user".to_string());
		let json = serde_json::to_string(&reason).unwrap();
		let parsed: MergeReason = serde_json::from_str(&json).unwrap();
		assert_eq!(reason, parsed);
	}

	proptest! {
		#[test]
		fn person_merge_id_is_unique(_seed: u64) {
			let id1 = PersonMergeId::new();
			let id2 = PersonMergeId::new();
			prop_assert_ne!(id1, id2);
		}

		#[test]
		fn person_merge_id_roundtrip(uuid_str in "[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}") {
			if let Ok(id) = uuid_str.parse::<PersonMergeId>() {
				let s = id.to_string();
				let parsed: PersonMergeId = s.parse().unwrap();
				prop_assert_eq!(id, parsed);
			}
		}

		#[test]
		fn identify_payload_serde_roundtrip(
			distinct_id in "[a-zA-Z0-9_]{1,50}",
			user_id in "[a-zA-Z0-9_@.]{1,50}",
		) {
			let payload = IdentifyPayload::new(distinct_id.clone(), user_id.clone());
			let json = serde_json::to_string(&payload).unwrap();
			let parsed: IdentifyPayload = serde_json::from_str(&json).unwrap();

			prop_assert_eq!(parsed.distinct_id, distinct_id);
			prop_assert_eq!(parsed.user_id, user_id);
		}

		#[test]
		fn alias_payload_serde_roundtrip(
			distinct_id in "[a-zA-Z0-9_]{1,50}",
			alias in "[a-zA-Z0-9_]{1,50}",
		) {
			let payload = AliasPayload::new(distinct_id.clone(), alias.clone());
			let json = serde_json::to_string(&payload).unwrap();
			let parsed: AliasPayload = serde_json::from_str(&json).unwrap();

			prop_assert_eq!(parsed.distinct_id, distinct_id);
			prop_assert_eq!(parsed.alias, alias);
		}

		#[test]
		fn merge_reason_identify_roundtrip(
			distinct_id in "[a-zA-Z0-9_]{1,50}",
			user_id in "[a-zA-Z0-9_@.]{1,50}",
		) {
			let reason = MergeReason::identify(distinct_id, user_id);
			let json = serde_json::to_string(&reason).unwrap();
			let parsed: MergeReason = serde_json::from_str(&json).unwrap();
			prop_assert_eq!(reason, parsed);
		}

		#[test]
		fn merge_reason_alias_roundtrip(
			distinct_id in "[a-zA-Z0-9_]{1,50}",
			alias in "[a-zA-Z0-9_]{1,50}",
		) {
			let reason = MergeReason::alias(distinct_id, alias);
			let json = serde_json::to_string(&reason).unwrap();
			let parsed: MergeReason = serde_json::from_str(&json).unwrap();
			prop_assert_eq!(reason, parsed);
		}
	}
}
