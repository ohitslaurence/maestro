// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Core types for the Loom product analytics system.
//!
//! This crate provides shared types for analytics including persons, events, identities,
//! and API keys. It is used by both the server-side storage and API layer
//! (`loom-server-analytics`) and the client SDK (`loom-analytics`).
//!
//! # Overview
//!
//! The analytics system supports:
//! - Person profiles for both anonymous and identified users
//! - Event tracking with flexible properties
//! - Identity resolution linking anonymous sessions to authenticated users
//! - Multi-tenant analytics scoped to organizations
//!
//! # Example
//!
//! ```
//! use loom_analytics_core::{
//!     Event, EventId, OrgId, Person, PersonId,
//!     IdentifyPayload, AnalyticsKeyType,
//! };
//!
//! // Create a person for an organization
//! let org_id = OrgId::new();
//! let person = Person::new(org_id);
//!
//! // Track an event
//! let event = Event::new(org_id, "user_123".to_string(), "button_clicked".to_string())
//!     .with_properties(serde_json::json!({"button_name": "checkout"}));
//!
//! // Link anonymous to authenticated
//! let identify = IdentifyPayload::new(
//!     "anon_abc123".to_string(),
//!     "user@example.com".to_string(),
//! ).with_properties(serde_json::json!({"plan": "pro"}));
//! ```

pub mod api_key;
pub mod error;
pub mod event;
pub mod identify;
pub mod identity;
pub mod person;

pub use api_key::{AnalyticsApiKey, AnalyticsApiKeyId, AnalyticsKeyType, UserId};
pub use error::{AnalyticsError, Result};
pub use event::{
	special_events, validate_event_name, validate_properties_size, Event, EventId,
	MAX_EVENT_NAME_LENGTH, MAX_PROPERTIES_SIZE,
};
pub use identify::{
	AliasPayload, IdentifyPayload, MergeReason, PersonMerge, PersonMergeId, SetOncePayload,
	SetPayload, UnsetPayload,
};
pub use identity::{
	validate_distinct_id, IdentityType, PersonIdentity, PersonIdentityId, MAX_DISTINCT_ID_LENGTH,
};
pub use person::{OrgId, Person, PersonId, PersonWithIdentities};

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn event_with_person_preserves_person_id(_seed: u64) {
			let org_id = OrgId::new();
			let person_id = PersonId::new();
			let event = Event::new(org_id, "user_123".to_string(), "click".to_string())
				.with_person_id(person_id);

			prop_assert_eq!(event.person_id, Some(person_id));
		}

		#[test]
		fn identify_links_distinct_id_to_user_id(
			distinct_id in "[a-zA-Z0-9_]{1,50}",
			user_id in "[a-zA-Z0-9_@.]{1,50}",
		) {
			let payload = IdentifyPayload::new(distinct_id.clone(), user_id.clone());
			prop_assert_eq!(payload.distinct_id, distinct_id);
			prop_assert_eq!(payload.user_id, user_id);
		}

		#[test]
		fn person_with_identities_detects_identified(
			has_identified in proptest::bool::ANY,
		) {
			let org_id = OrgId::new();
			let person = Person::new(org_id);
			let person_id = person.id;

			let identities = if has_identified {
				vec![
					PersonIdentity::anonymous(person_id, "anon_123".to_string()),
					PersonIdentity::identified(person_id, "user@example.com".to_string()),
				]
			} else {
				vec![
					PersonIdentity::anonymous(person_id, "anon_123".to_string()),
					PersonIdentity::anonymous(person_id, "anon_456".to_string()),
				]
			};

			let pwi = PersonWithIdentities::new(person, identities);
			prop_assert_eq!(pwi.has_identified_identity(), has_identified);
		}

		#[test]
		fn api_key_permissions_are_consistent(is_write in proptest::bool::ANY) {
			let key_type = if is_write {
				AnalyticsKeyType::Write
			} else {
				AnalyticsKeyType::ReadWrite
			};

			prop_assert!(key_type.can_capture());
			if is_write {
				prop_assert!(!key_type.can_query());
			} else {
				prop_assert!(key_type.can_query());
			}
		}
	}
}
