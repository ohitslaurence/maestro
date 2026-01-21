// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Organization management for multi-tenant access control.
//!
//! Every thread belongs to an organization. Each user automatically gets a "Personal" org.
//! Organizations support multiple owners, admins, and members with role-based permissions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{InvitationId, OrgId, OrgRole, UserId};

// =============================================================================
// Organization Visibility
// =============================================================================

/// Visibility setting for an organization.
///
/// - `Public`: Visible in org directory, anyone can request to join (default)
/// - `Unlisted`: Not in directory, can request if they know org name
/// - `Private`: Invisible, only direct add works
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrgVisibility {
	#[default]
	Public,
	Unlisted,
	Private,
}

impl std::fmt::Display for OrgVisibility {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			OrgVisibility::Public => write!(f, "public"),
			OrgVisibility::Unlisted => write!(f, "unlisted"),
			OrgVisibility::Private => write!(f, "private"),
		}
	}
}

// =============================================================================
// Organization
// =============================================================================

/// An organization that owns threads and has members.
///
/// Every thread belongs to an organization. Each user automatically gets a "Personal" org
/// which is marked with `is_personal = true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Organization {
	pub id: OrgId,
	pub name: String,
	pub slug: String,
	pub visibility: OrgVisibility,
	pub is_personal: bool,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
	pub deleted_at: Option<DateTime<Utc>>,
}

impl Organization {
	/// Create a new regular (non-personal) organization.
	pub fn new(name: impl Into<String>, slug: impl Into<String>) -> Self {
		let now = Utc::now();
		Self {
			id: OrgId::generate(),
			name: name.into(),
			slug: slug.into(),
			visibility: OrgVisibility::default(),
			is_personal: false,
			created_at: now,
			updated_at: now,
			deleted_at: None,
		}
	}

	/// Create a personal organization for a user.
	///
	/// Personal orgs are auto-created when a user signs up. The org name is set to
	/// "{user_id}'s Personal" and the slug is derived from the user ID.
	pub fn new_personal(owner: &UserId) -> Self {
		let now = Utc::now();
		Self {
			id: OrgId::generate(),
			name: format!("{owner}'s Personal"),
			slug: format!("personal-{owner}"),
			visibility: OrgVisibility::Public,
			is_personal: true,
			created_at: now,
			updated_at: now,
			deleted_at: None,
		}
	}

	/// Returns true if this organization has been soft-deleted.
	pub fn is_deleted(&self) -> bool {
		self.deleted_at.is_some()
	}

	/// Returns true if this is a personal organization.
	pub fn is_personal(&self) -> bool {
		self.is_personal
	}
}

// =============================================================================
// Organization Membership
// =============================================================================

/// A user's membership in an organization.
///
/// Tracks which users belong to which orgs and their role within each org.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgMembership {
	pub org_id: OrgId,
	pub user_id: UserId,
	pub role: OrgRole,
	pub created_at: DateTime<Utc>,
}

impl OrgMembership {
	/// Create a new organization membership.
	pub fn new(org_id: OrgId, user_id: UserId, role: OrgRole) -> Self {
		Self {
			org_id,
			user_id,
			role,
			created_at: Utc::now(),
		}
	}

	/// Create an owner membership for a new organization.
	pub fn new_owner(org_id: OrgId, user_id: UserId) -> Self {
		Self::new(org_id, user_id, OrgRole::Owner)
	}
}

// =============================================================================
// Organization Invitation
// =============================================================================

/// An invitation to join an organization.
///
/// Invitations are sent via email and expire after 30 days. The token is stored
/// as a hash for security.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgInvitation {
	pub id: InvitationId,
	pub org_id: OrgId,
	pub email: String,
	pub role: OrgRole,
	pub invited_by: UserId,
	pub token_hash: String,
	pub created_at: DateTime<Utc>,
	pub expires_at: DateTime<Utc>,
	pub accepted_at: Option<DateTime<Utc>>,
}

impl OrgInvitation {
	/// Default invitation expiry duration (30 days).
	pub const EXPIRY_DAYS: i64 = 30;

	/// Create a new invitation.
	///
	/// The `token_hash` should be the Argon2 hash of the invitation token.
	/// The actual token is only shown to the inviter once.
	pub fn new(
		org_id: OrgId,
		email: impl Into<String>,
		role: OrgRole,
		invited_by: UserId,
		token_hash: impl Into<String>,
	) -> Self {
		let now = Utc::now();
		Self {
			id: InvitationId::generate(),
			org_id,
			email: email.into(),
			role,
			invited_by,
			token_hash: token_hash.into(),
			created_at: now,
			expires_at: now + chrono::Duration::days(Self::EXPIRY_DAYS),
			accepted_at: None,
		}
	}

	/// Returns true if this invitation has expired.
	pub fn is_expired(&self) -> bool {
		Utc::now() > self.expires_at
	}

	/// Returns true if this invitation has been accepted.
	pub fn is_accepted(&self) -> bool {
		self.accepted_at.is_some()
	}

	/// Returns true if this invitation is still valid (not expired and not accepted).
	pub fn is_valid(&self) -> bool {
		!self.is_expired() && !self.is_accepted()
	}
}

// =============================================================================
// Organization Join Request
// =============================================================================

/// A request from a user to join an organization.
///
/// For public and unlisted orgs, users can request to join and admins can approve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgJoinRequest {
	pub org_id: OrgId,
	pub user_id: UserId,
	pub created_at: DateTime<Utc>,
	pub handled_at: Option<DateTime<Utc>>,
	pub handled_by: Option<UserId>,
	pub approved: Option<bool>,
}

impl OrgJoinRequest {
	/// Create a new join request.
	pub fn new(org_id: OrgId, user_id: UserId) -> Self {
		Self {
			org_id,
			user_id,
			created_at: Utc::now(),
			handled_at: None,
			handled_by: None,
			approved: None,
		}
	}

	/// Returns true if this request is still pending.
	pub fn is_pending(&self) -> bool {
		self.handled_at.is_none()
	}

	/// Returns true if this request was approved.
	pub fn is_approved(&self) -> bool {
		self.approved == Some(true)
	}

	/// Returns true if this request was rejected.
	pub fn is_rejected(&self) -> bool {
		self.approved == Some(false)
	}
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
	use super::*;

	mod org_visibility {
		use super::*;

		#[test]
		fn default_is_public() {
			assert_eq!(OrgVisibility::default(), OrgVisibility::Public);
		}

		#[test]
		fn serializes_snake_case() {
			assert_eq!(
				serde_json::to_string(&OrgVisibility::Public).unwrap(),
				"\"public\""
			);
			assert_eq!(
				serde_json::to_string(&OrgVisibility::Unlisted).unwrap(),
				"\"unlisted\""
			);
			assert_eq!(
				serde_json::to_string(&OrgVisibility::Private).unwrap(),
				"\"private\""
			);
		}

		#[test]
		fn display_matches_serialization() {
			assert_eq!(OrgVisibility::Public.to_string(), "public");
			assert_eq!(OrgVisibility::Unlisted.to_string(), "unlisted");
			assert_eq!(OrgVisibility::Private.to_string(), "private");
		}
	}

	mod organization {
		use super::*;

		#[test]
		fn new_creates_regular_org() {
			let org = Organization::new("Acme Corp", "acme-corp");

			assert_eq!(org.name, "Acme Corp");
			assert_eq!(org.slug, "acme-corp");
			assert_eq!(org.visibility, OrgVisibility::Public);
			assert!(!org.is_personal());
			assert!(!org.is_deleted());
		}

		#[test]
		fn new_personal_creates_personal_org() {
			let user_id = UserId::generate();
			let org = Organization::new_personal(&user_id);

			assert!(org.name.contains("Personal"));
			assert!(org.slug.starts_with("personal-"));
			assert!(org.is_personal());
			assert!(!org.is_deleted());
		}

		#[test]
		fn is_deleted_reflects_deleted_at() {
			let mut org = Organization::new("Test", "test");
			assert!(!org.is_deleted());

			org.deleted_at = Some(Utc::now());
			assert!(org.is_deleted());
		}

		#[test]
		fn serializes_correctly() {
			let org = Organization::new("Test Org", "test-org");
			let json = serde_json::to_string(&org).unwrap();

			assert!(json.contains("\"name\":\"Test Org\""));
			assert!(json.contains("\"slug\":\"test-org\""));
			assert!(json.contains("\"visibility\":\"public\""));
			assert!(json.contains("\"is_personal\":false"));
		}
	}

	mod org_membership {
		use super::*;

		#[test]
		fn new_creates_membership() {
			let org_id = OrgId::generate();
			let user_id = UserId::generate();
			let membership = OrgMembership::new(org_id, user_id, OrgRole::Member);

			assert_eq!(membership.org_id, org_id);
			assert_eq!(membership.user_id, user_id);
			assert_eq!(membership.role, OrgRole::Member);
		}

		#[test]
		fn new_owner_creates_owner_membership() {
			let org_id = OrgId::generate();
			let user_id = UserId::generate();
			let membership = OrgMembership::new_owner(org_id, user_id);

			assert_eq!(membership.role, OrgRole::Owner);
		}
	}

	mod org_invitation {
		use super::*;

		#[test]
		fn new_creates_valid_invitation() {
			let org_id = OrgId::generate();
			let inviter = UserId::generate();
			let invitation = OrgInvitation::new(
				org_id,
				"user@example.com",
				OrgRole::Member,
				inviter,
				"hashed_token",
			);

			assert_eq!(invitation.org_id, org_id);
			assert_eq!(invitation.email, "user@example.com");
			assert_eq!(invitation.role, OrgRole::Member);
			assert!(invitation.is_valid());
			assert!(!invitation.is_expired());
			assert!(!invitation.is_accepted());
		}

		#[test]
		fn expires_after_30_days() {
			let org_id = OrgId::generate();
			let inviter = UserId::generate();
			let mut invitation = OrgInvitation::new(
				org_id,
				"user@example.com",
				OrgRole::Member,
				inviter,
				"hashed_token",
			);

			// Simulate expiry
			invitation.expires_at = Utc::now() - chrono::Duration::hours(1);
			assert!(invitation.is_expired());
			assert!(!invitation.is_valid());
		}

		#[test]
		fn accepted_invitation_not_valid() {
			let org_id = OrgId::generate();
			let inviter = UserId::generate();
			let mut invitation = OrgInvitation::new(
				org_id,
				"user@example.com",
				OrgRole::Member,
				inviter,
				"hashed_token",
			);

			invitation.accepted_at = Some(Utc::now());
			assert!(invitation.is_accepted());
			assert!(!invitation.is_valid());
		}
	}

	mod org_join_request {
		use super::*;

		#[test]
		fn new_creates_pending_request() {
			let org_id = OrgId::generate();
			let user_id = UserId::generate();
			let request = OrgJoinRequest::new(org_id, user_id);

			assert_eq!(request.org_id, org_id);
			assert_eq!(request.user_id, user_id);
			assert!(request.is_pending());
			assert!(!request.is_approved());
			assert!(!request.is_rejected());
		}

		#[test]
		fn approved_request_state() {
			let org_id = OrgId::generate();
			let user_id = UserId::generate();
			let handler = UserId::generate();
			let mut request = OrgJoinRequest::new(org_id, user_id);

			request.handled_at = Some(Utc::now());
			request.handled_by = Some(handler);
			request.approved = Some(true);

			assert!(!request.is_pending());
			assert!(request.is_approved());
			assert!(!request.is_rejected());
		}

		#[test]
		fn rejected_request_state() {
			let org_id = OrgId::generate();
			let user_id = UserId::generate();
			let handler = UserId::generate();
			let mut request = OrgJoinRequest::new(org_id, user_id);

			request.handled_at = Some(Utc::now());
			request.handled_by = Some(handler);
			request.approved = Some(false);

			assert!(!request.is_pending());
			assert!(!request.is_approved());
			assert!(request.is_rejected());
		}
	}
}
