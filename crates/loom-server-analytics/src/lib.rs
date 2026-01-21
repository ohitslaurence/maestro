// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Product analytics server implementation for Loom.
//!
//! This crate provides the server-side implementation for the analytics system,
//! including database operations, person management, event storage, and API key management.
//!
//! # Architecture
//!
//! - `repository` - Database operations for persons, events, identities, and API keys
//! - `api_key` - API key hashing and verification using Argon2
//!
//! # Example
//!
//! ```ignore
//! use loom_server_analytics::{SqliteAnalyticsRepository, AnalyticsRepository};
//! use loom_analytics_core::{Event, Person, OrgId};
//!
//! // Create repository
//! let repo = SqliteAnalyticsRepository::new(pool);
//!
//! // Create a person
//! let org_id = OrgId::new();
//! let person = Person::new(org_id);
//! repo.create_person(&person).await?;
//!
//! // Track an event
//! let event = Event::new(org_id, "user_123".to_string(), "button_clicked".to_string());
//! repo.insert_event(&event).await?;
//!
//! // Query events
//! let events = repo.list_events(org_id, None, None, None, None, 100, 0).await?;
//! ```

pub mod api_key;
pub mod error;
pub mod handlers;
pub mod identity_resolution;
pub mod middleware;
pub mod repository;
pub mod routes;

pub use api_key::{hash_api_key, verify_api_key};
pub use error::{AnalyticsServerError, Result};
pub use handlers::api_keys::{
	api_key_to_response, api_key_type_from_api, api_key_type_to_api, create_api_key_impl,
	generate_api_key, list_api_keys_impl, revoke_api_key_impl, UserAuthContext,
};
pub use handlers::capture::{batch_capture_impl, capture_event_impl, AnalyticsState};
pub use handlers::events::{count_events_impl, export_events_impl, list_events_impl};
pub use handlers::identify::{alias_impl, identify_impl, set_properties_impl};
pub use handlers::persons::{
	get_person_by_distinct_id_impl, get_person_impl, list_persons_impl, person_to_response,
};
pub use identity_resolution::{
	IdentityResolutionService, MergeAuditHook, NoOpMergeAuditHook, PersonMergeDetails,
	SharedMergeAuditHook,
};
pub use middleware::{
	extract_bearer_token, parse_key_type, AnalyticsApiKeyContext, AnalyticsApiKeyError,
	READ_WRITE_KEY_PREFIX, WRITE_KEY_PREFIX,
};
pub use repository::{AnalyticsRepository, SqliteAnalyticsRepository};

// Re-export core types for convenience
pub use loom_analytics_core::*;
