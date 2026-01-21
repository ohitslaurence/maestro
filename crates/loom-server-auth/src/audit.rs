// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Audit logging for authentication and authorization events.
//!
//! This module provides comprehensive audit logging for security-relevant events
//! including authentication, session management, API key operations, access control
//! decisions, and administrative actions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::UserId;

/// Retention period for audit logs in days.
pub const AUDIT_RETENTION_DAYS: i64 = 90;

/// Types of events that can be recorded in the audit log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
	// Authentication events
	/// User successfully logged in.
	Login,
	/// User logged out.
	Logout,
	/// Login attempt failed.
	LoginFailed,

	// Session events
	/// New session was created.
	SessionCreated,
	/// Session was revoked by user or admin.
	SessionRevoked,
	/// Session expired naturally.
	SessionExpired,

	// API key events
	/// API key was created.
	ApiKeyCreated,
	/// API key was used for authentication.
	ApiKeyUsed,
	/// API key was revoked.
	ApiKeyRevoked,

	// Access control events
	/// Access to a resource was granted.
	AccessGranted,
	/// Access to a resource was denied.
	AccessDenied,

	// Organization membership events
	/// Member was added to an organization.
	MemberAdded,
	/// Member was removed from an organization.
	MemberRemoved,
	/// Member's role was changed.
	RoleChanged,

	// Impersonation events
	/// Admin started impersonating a user.
	ImpersonationStarted,
	/// Admin ended impersonation session.
	ImpersonationEnded,

	// Organization lifecycle events
	/// Organization was created.
	OrgCreated,
	/// Organization was deleted (soft delete).
	OrgDeleted,
	/// Organization was restored from deletion.
	OrgRestored,

	// Thread sharing events
	/// Thread was shared with users/teams/org.
	ThreadShared,
	/// Thread sharing was revoked.
	ThreadUnshared,

	// Support access events
	/// Support access was requested for a user.
	SupportAccessRequested,
	/// Support access was approved by user.
	SupportAccessApproved,
	/// Support access was revoked.
	SupportAccessRevoked,
}

impl std::fmt::Display for AuditEventType {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let s = match self {
			AuditEventType::Login => "login",
			AuditEventType::Logout => "logout",
			AuditEventType::LoginFailed => "login_failed",
			AuditEventType::SessionCreated => "session_created",
			AuditEventType::SessionRevoked => "session_revoked",
			AuditEventType::SessionExpired => "session_expired",
			AuditEventType::ApiKeyCreated => "api_key_created",
			AuditEventType::ApiKeyUsed => "api_key_used",
			AuditEventType::ApiKeyRevoked => "api_key_revoked",
			AuditEventType::AccessGranted => "access_granted",
			AuditEventType::AccessDenied => "access_denied",
			AuditEventType::MemberAdded => "member_added",
			AuditEventType::MemberRemoved => "member_removed",
			AuditEventType::RoleChanged => "role_changed",
			AuditEventType::ImpersonationStarted => "impersonation_started",
			AuditEventType::ImpersonationEnded => "impersonation_ended",
			AuditEventType::OrgCreated => "org_created",
			AuditEventType::OrgDeleted => "org_deleted",
			AuditEventType::OrgRestored => "org_restored",
			AuditEventType::ThreadShared => "thread_shared",
			AuditEventType::ThreadUnshared => "thread_unshared",
			AuditEventType::SupportAccessRequested => "support_access_requested",
			AuditEventType::SupportAccessApproved => "support_access_approved",
			AuditEventType::SupportAccessRevoked => "support_access_revoked",
		};
		write!(f, "{s}")
	}
}

/// An entry in the audit log recording a security-relevant event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
	/// Unique identifier for this audit entry.
	pub id: Uuid,
	/// When the event occurred.
	pub timestamp: DateTime<Utc>,
	/// The type of event.
	pub event_type: AuditEventType,
	/// The user who performed the action (if known).
	pub actor_user_id: Option<UserId>,
	/// If the actor is impersonating another user, this is the real admin's ID.
	pub impersonating_user_id: Option<UserId>,
	/// The type of resource affected (e.g., "thread", "organization", "session").
	pub resource_type: Option<String>,
	/// The ID of the resource affected.
	pub resource_id: Option<String>,
	/// Human-readable description of the action.
	pub action: String,
	/// IP address of the request origin.
	pub ip_address: Option<String>,
	/// User agent string from the request.
	pub user_agent: Option<String>,
	/// Additional event-specific details.
	pub details: serde_json::Value,
}

impl AuditLogEntry {
	/// Create a new audit log builder for the given event type.
	pub fn builder(event_type: AuditEventType) -> AuditLogBuilder {
		AuditLogBuilder::new(event_type)
	}
}

/// Builder for constructing audit log entries with a fluent API.
#[derive(Debug, Clone)]
pub struct AuditLogBuilder {
	event_type: AuditEventType,
	actor_user_id: Option<UserId>,
	impersonating_user_id: Option<UserId>,
	resource_type: Option<String>,
	resource_id: Option<String>,
	action: Option<String>,
	ip_address: Option<String>,
	user_agent: Option<String>,
	details: serde_json::Value,
}

impl AuditLogBuilder {
	/// Create a new builder for the given event type.
	pub fn new(event_type: AuditEventType) -> Self {
		Self {
			event_type,
			actor_user_id: None,
			impersonating_user_id: None,
			resource_type: None,
			resource_id: None,
			action: None,
			ip_address: None,
			user_agent: None,
			details: serde_json::Value::Null,
		}
	}

	/// Set the user who performed the action.
	pub fn actor(mut self, user_id: UserId) -> Self {
		self.actor_user_id = Some(user_id);
		self
	}

	/// Set the real admin's ID if the actor is impersonating another user.
	pub fn impersonating(mut self, admin_user_id: UserId) -> Self {
		self.impersonating_user_id = Some(admin_user_id);
		self
	}

	/// Set the resource type and ID affected by this event.
	pub fn resource(
		mut self,
		resource_type: impl Into<String>,
		resource_id: impl Into<String>,
	) -> Self {
		self.resource_type = Some(resource_type.into());
		self.resource_id = Some(resource_id.into());
		self
	}

	/// Set the human-readable action description.
	pub fn action(mut self, action: impl Into<String>) -> Self {
		self.action = Some(action.into());
		self
	}

	/// Set the IP address of the request origin.
	pub fn ip_address(mut self, ip: impl Into<String>) -> Self {
		self.ip_address = Some(ip.into());
		self
	}

	/// Set the user agent string from the request.
	pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
		self.user_agent = Some(ua.into());
		self
	}

	/// Set additional event-specific details.
	pub fn details(mut self, details: serde_json::Value) -> Self {
		self.details = details;
		self
	}

	/// Build the audit log entry.
	pub fn build(self) -> AuditLogEntry {
		AuditLogEntry {
			id: Uuid::new_v4(),
			timestamp: Utc::now(),
			event_type: self.event_type,
			actor_user_id: self.actor_user_id,
			impersonating_user_id: self.impersonating_user_id,
			resource_type: self.resource_type,
			resource_id: self.resource_id,
			action: self.action.unwrap_or_else(|| self.event_type.to_string()),
			ip_address: self.ip_address,
			user_agent: self.user_agent,
			details: self.details,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	mod audit_event_type {
		use super::*;

		#[test]
		fn display_returns_snake_case() {
			assert_eq!(AuditEventType::Login.to_string(), "login");
			assert_eq!(AuditEventType::LoginFailed.to_string(), "login_failed");
			assert_eq!(
				AuditEventType::SessionCreated.to_string(),
				"session_created"
			);
			assert_eq!(
				AuditEventType::ImpersonationStarted.to_string(),
				"impersonation_started"
			);
			assert_eq!(
				AuditEventType::SupportAccessApproved.to_string(),
				"support_access_approved"
			);
		}

		#[test]
		fn serializes_snake_case() {
			let event = AuditEventType::ApiKeyCreated;
			let json = serde_json::to_string(&event).unwrap();
			assert_eq!(json, "\"api_key_created\"");
		}

		#[test]
		fn deserializes_snake_case() {
			let event: AuditEventType = serde_json::from_str("\"access_denied\"").unwrap();
			assert_eq!(event, AuditEventType::AccessDenied);
		}

		#[test]
		fn all_event_types_serialize_deserialize() {
			let events = [
				AuditEventType::Login,
				AuditEventType::Logout,
				AuditEventType::LoginFailed,
				AuditEventType::SessionCreated,
				AuditEventType::SessionRevoked,
				AuditEventType::SessionExpired,
				AuditEventType::ApiKeyCreated,
				AuditEventType::ApiKeyUsed,
				AuditEventType::ApiKeyRevoked,
				AuditEventType::AccessGranted,
				AuditEventType::AccessDenied,
				AuditEventType::MemberAdded,
				AuditEventType::MemberRemoved,
				AuditEventType::RoleChanged,
				AuditEventType::ImpersonationStarted,
				AuditEventType::ImpersonationEnded,
				AuditEventType::OrgCreated,
				AuditEventType::OrgDeleted,
				AuditEventType::OrgRestored,
				AuditEventType::ThreadShared,
				AuditEventType::ThreadUnshared,
				AuditEventType::SupportAccessRequested,
				AuditEventType::SupportAccessApproved,
				AuditEventType::SupportAccessRevoked,
			];

			for event in events {
				let json = serde_json::to_string(&event).unwrap();
				let roundtrip: AuditEventType = serde_json::from_str(&json).unwrap();
				assert_eq!(event, roundtrip);
			}
		}
	}

	mod audit_log_entry {
		use super::*;

		#[test]
		fn new_returns_builder() {
			let builder = AuditLogEntry::builder(AuditEventType::Login);
			let entry = builder.build();
			assert_eq!(entry.event_type, AuditEventType::Login);
		}

		#[test]
		fn serializes_to_json() {
			let user_id = UserId::generate();
			let entry = AuditLogEntry::builder(AuditEventType::Login)
				.actor(user_id)
				.ip_address("192.168.1.1")
				.build();

			let json = serde_json::to_string(&entry).unwrap();
			assert!(json.contains("\"event_type\":\"login\""));
			assert!(json.contains("\"ip_address\":\"192.168.1.1\""));
		}

		#[test]
		fn deserializes_from_json() {
			let user_id = UserId::generate();
			let original = AuditLogEntry::builder(AuditEventType::AccessDenied)
				.actor(user_id)
				.resource("thread", "thread-123")
				.action("User attempted to access private thread")
				.build();

			let json = serde_json::to_string(&original).unwrap();
			let restored: AuditLogEntry = serde_json::from_str(&json).unwrap();

			assert_eq!(restored.id, original.id);
			assert_eq!(restored.event_type, AuditEventType::AccessDenied);
			assert_eq!(restored.resource_type, Some("thread".to_string()));
			assert_eq!(restored.resource_id, Some("thread-123".to_string()));
		}
	}

	mod audit_log_builder {
		use super::*;
		use serde_json::json;

		#[test]
		fn builds_minimal_entry() {
			let entry = AuditLogBuilder::new(AuditEventType::Logout).build();

			assert_eq!(entry.event_type, AuditEventType::Logout);
			assert!(entry.actor_user_id.is_none());
			assert!(entry.impersonating_user_id.is_none());
			assert!(entry.resource_type.is_none());
			assert!(entry.resource_id.is_none());
			assert_eq!(entry.action, "logout");
			assert!(entry.ip_address.is_none());
			assert!(entry.user_agent.is_none());
			assert_eq!(entry.details, serde_json::Value::Null);
		}

		#[test]
		fn builds_full_entry() {
			let actor = UserId::generate();
			let admin = UserId::generate();

			let entry = AuditLogBuilder::new(AuditEventType::RoleChanged)
				.actor(actor)
				.impersonating(admin)
				.resource("org_membership", "mem-456")
				.action("Changed role from member to admin")
				.ip_address("10.0.0.1")
				.user_agent("Mozilla/5.0")
				.details(json!({"old_role": "member", "new_role": "admin"}))
				.build();

			assert_eq!(entry.event_type, AuditEventType::RoleChanged);
			assert_eq!(entry.actor_user_id, Some(actor));
			assert_eq!(entry.impersonating_user_id, Some(admin));
			assert_eq!(entry.resource_type, Some("org_membership".to_string()));
			assert_eq!(entry.resource_id, Some("mem-456".to_string()));
			assert_eq!(entry.action, "Changed role from member to admin");
			assert_eq!(entry.ip_address, Some("10.0.0.1".to_string()));
			assert_eq!(entry.user_agent, Some("Mozilla/5.0".to_string()));
			assert_eq!(entry.details["old_role"], "member");
			assert_eq!(entry.details["new_role"], "admin");
		}

		#[test]
		fn generates_unique_ids() {
			let entry1 = AuditLogBuilder::new(AuditEventType::Login).build();
			let entry2 = AuditLogBuilder::new(AuditEventType::Login).build();
			assert_ne!(entry1.id, entry2.id);
		}

		#[test]
		fn sets_timestamp_to_now() {
			let before = Utc::now();
			let entry = AuditLogBuilder::new(AuditEventType::Login).build();
			let after = Utc::now();

			assert!(entry.timestamp >= before);
			assert!(entry.timestamp <= after);
		}

		#[test]
		fn default_action_uses_event_type_display() {
			let entry = AuditLogBuilder::new(AuditEventType::ApiKeyRevoked).build();
			assert_eq!(entry.action, "api_key_revoked");
		}

		#[test]
		fn custom_action_overrides_default() {
			let entry = AuditLogBuilder::new(AuditEventType::ApiKeyRevoked)
				.action("Admin revoked API key due to suspected compromise")
				.build();
			assert_eq!(
				entry.action,
				"Admin revoked API key due to suspected compromise"
			);
		}

		#[test]
		fn impersonation_tracking() {
			let target_user = UserId::generate();
			let admin_user = UserId::generate();

			let entry = AuditLogBuilder::new(AuditEventType::ImpersonationStarted)
				.actor(target_user)
				.impersonating(admin_user)
				.details(json!({"reason": "Investigating reported issue"}))
				.build();

			assert_eq!(entry.actor_user_id, Some(target_user));
			assert_eq!(entry.impersonating_user_id, Some(admin_user));
		}
	}

	mod constants {
		use super::*;

		#[test]
		fn retention_days_is_90() {
			assert_eq!(AUDIT_RETENTION_DAYS, 90);
		}
	}
}
