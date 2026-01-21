// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Person types for user profile management.
//!
//! A [`Person`] represents a user profile that can have multiple identities
//! (anonymous sessions and authenticated IDs) linked to it via identity resolution.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::identity::PersonIdentity;

/// Unique identifier for a person profile.
///
/// Uses UUIDv4 for random generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PersonId(pub Uuid);

impl PersonId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for PersonId {
	fn default() -> Self {
		Self::new()
	}
}

impl std::fmt::Display for PersonId {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl std::str::FromStr for PersonId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// Unique identifier for an organization.
///
/// Organizations own persons, events, and API keys. All analytics data
/// is scoped to a single organization for multi-tenant isolation.
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

/// A person profile in the analytics system.
///
/// Persons are the central entity for identity resolution. Each person belongs
/// to an organization and can have multiple identities (anonymous and identified)
/// linked to them. Persons can be merged when identity resolution determines
/// two persons represent the same user.
///
/// # Properties
///
/// The `properties` field stores arbitrary JSON data about the person,
/// such as name, email, plan, or custom attributes set via [`set_property`](Self::set_property).
///
/// # Merging
///
/// When two persons are determined to be the same user (via identify or alias),
/// one is merged into the other. The "loser" has `merged_into_id` set and
/// their events/identities are transferred to the "winner".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Person {
	pub id: PersonId,
	pub org_id: OrgId,
	pub properties: serde_json::Value,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
	pub merged_into_id: Option<PersonId>,
	pub merged_at: Option<DateTime<Utc>>,
}

impl Person {
	/// Creates a new person in the given organization with empty properties.
	pub fn new(org_id: OrgId) -> Self {
		let now = Utc::now();
		Self {
			id: PersonId::new(),
			org_id,
			properties: serde_json::json!({}),
			created_at: now,
			updated_at: now,
			merged_into_id: None,
			merged_at: None,
		}
	}

	/// Sets the initial properties for this person (builder pattern).
	pub fn with_properties(mut self, properties: serde_json::Value) -> Self {
		self.properties = properties;
		self
	}

	/// Returns `true` if this person has been merged into another.
	pub fn is_merged(&self) -> bool {
		self.merged_into_id.is_some()
	}

	/// Sets a single property, overwriting any existing value.
	pub fn set_property(&mut self, key: &str, value: serde_json::Value) {
		if let serde_json::Value::Object(ref mut map) = self.properties {
			map.insert(key.to_string(), value);
		}
		self.updated_at = Utc::now();
	}

	/// Merges multiple properties into this person, overwriting existing keys.
	pub fn set_properties(&mut self, properties: serde_json::Value) {
		if let (serde_json::Value::Object(ref mut existing), serde_json::Value::Object(new)) =
			(&mut self.properties, properties)
		{
			for (key, value) in new {
				existing.insert(key, value);
			}
		}
		self.updated_at = Utc::now();
	}

	/// Sets a property only if it doesn't already exist.
	pub fn set_property_once(&mut self, key: &str, value: serde_json::Value) {
		if let serde_json::Value::Object(ref mut map) = self.properties {
			map.entry(key.to_string()).or_insert(value);
		}
		self.updated_at = Utc::now();
	}

	/// Removes a property from this person.
	pub fn unset_property(&mut self, key: &str) {
		if let serde_json::Value::Object(ref mut map) = self.properties {
			map.remove(key);
		}
		self.updated_at = Utc::now();
	}

	/// Marks this person as merged into another (the "winner").
	pub fn merge_into(&mut self, winner_id: PersonId) {
		self.merged_into_id = Some(winner_id);
		self.merged_at = Some(Utc::now());
		self.updated_at = Utc::now();
	}
}

/// A person bundled with their linked identities.
///
/// This is a convenience type for queries that need both the person
/// profile and their associated distinct IDs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonWithIdentities {
	/// The person profile.
	pub person: Person,
	/// All identities (anonymous and identified) linked to this person.
	pub identities: Vec<PersonIdentity>,
}

impl PersonWithIdentities {
	/// Creates a new person-with-identities bundle.
	pub fn new(person: Person, identities: Vec<PersonIdentity>) -> Self {
		Self { person, identities }
	}

	/// Returns `true` if any identity is of type `Identified`.
	pub fn has_identified_identity(&self) -> bool {
		use crate::identity::IdentityType;
		self
			.identities
			.iter()
			.any(|i| i.identity_type == IdentityType::Identified)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	#[test]
	fn person_id_roundtrip() {
		let id = PersonId::new();
		let s = id.to_string();
		let parsed: PersonId = s.parse().unwrap();
		assert_eq!(id, parsed);
	}

	#[test]
	fn org_id_roundtrip() {
		let id = OrgId::new();
		let s = id.to_string();
		let parsed: OrgId = s.parse().unwrap();
		assert_eq!(id, parsed);
	}

	#[test]
	fn person_new_has_empty_properties() {
		let person = Person::new(OrgId::new());
		assert_eq!(person.properties, serde_json::json!({}));
		assert!(!person.is_merged());
	}

	#[test]
	fn person_set_property() {
		let mut person = Person::new(OrgId::new());
		person.set_property("name", serde_json::json!("Alice"));
		assert_eq!(person.properties["name"], "Alice");
	}

	#[test]
	fn person_set_property_once_does_not_overwrite() {
		let mut person = Person::new(OrgId::new());
		person.set_property("name", serde_json::json!("Alice"));
		person.set_property_once("name", serde_json::json!("Bob"));
		assert_eq!(person.properties["name"], "Alice");
	}

	#[test]
	fn person_unset_property() {
		let mut person = Person::new(OrgId::new());
		person.set_property("name", serde_json::json!("Alice"));
		person.unset_property("name");
		assert!(person.properties.get("name").is_none());
	}

	#[test]
	fn person_merge_into() {
		let mut loser = Person::new(OrgId::new());
		let winner_id = PersonId::new();
		loser.merge_into(winner_id);
		assert!(loser.is_merged());
		assert_eq!(loser.merged_into_id, Some(winner_id));
		assert!(loser.merged_at.is_some());
	}

	proptest! {
		#[test]
		fn person_id_is_unique(_seed: u64) {
			let id1 = PersonId::new();
			let id2 = PersonId::new();
			prop_assert_ne!(id1, id2);
		}

		#[test]
		fn org_id_is_unique(_seed: u64) {
			let id1 = OrgId::new();
			let id2 = OrgId::new();
			prop_assert_ne!(id1, id2);
		}

		#[test]
		fn person_id_parse_roundtrip(uuid_str in "[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}") {
			if let Ok(id) = uuid_str.parse::<PersonId>() {
				let s = id.to_string();
				let parsed: PersonId = s.parse().unwrap();
				prop_assert_eq!(id, parsed);
			}
		}

		#[test]
		fn person_set_properties_merges(
			key1 in "[a-z]{1,10}",
			key2 in "[a-z]{1,10}",
			val1 in "[a-z]{1,10}",
			val2 in "[a-z]{1,10}",
		) {
			let mut person = Person::new(OrgId::new());
			person.set_property(&key1, serde_json::json!(val1));
			person.set_properties(serde_json::json!({ key2.clone(): val2.clone() }));

			if key1 != key2 {
				prop_assert_eq!(&person.properties[&key1], &serde_json::json!(val1));
				prop_assert_eq!(&person.properties[&key2], &serde_json::json!(val2));
			}
		}
	}
}
