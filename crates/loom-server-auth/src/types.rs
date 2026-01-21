// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Core type definitions for authentication and authorization.
//!
//! This module defines the foundational types used throughout the auth system:
//!
//! - **ID newtypes**: Type-safe wrappers around UUIDs for different entity types
//!   ([`UserId`], [`SessionId`], [`OrgId`], etc.) preventing accidental mixing
//! - **Role enums**: Hierarchical roles for global ([`GlobalRole`]), organization
//!   ([`OrgRole`]), and team ([`TeamRole`]) scopes
//! - **Session types**: Classification of authentication methods ([`SessionType`])
//! - **Visibility levels**: Access control for resources ([`Visibility`])
//! - **API key scopes**: Fine-grained permissions for programmatic access ([`ApiKeyScope`])
//!
//! All ID types implement transparent serde serialization (as UUID strings) and
//! provide conversion to/from [`uuid::Uuid`].

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

// =============================================================================
// ID Newtypes
// =============================================================================

macro_rules! define_id_type {
	($name:ident, $doc:expr) => {
		#[doc = $doc]
		#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
		#[serde(transparent)]
		pub struct $name(Uuid);

		impl $name {
			/// Create a new ID from a UUID.
			pub fn new(id: Uuid) -> Self {
				Self(id)
			}

			/// Generate a new random ID.
			pub fn generate() -> Self {
				Self(Uuid::new_v4())
			}

			/// Get the inner UUID value.
			pub fn into_inner(self) -> Uuid {
				self.0
			}

			/// Get a reference to the inner UUID.
			pub fn as_uuid(&self) -> &Uuid {
				&self.0
			}
		}

		impl fmt::Display for $name {
			fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
				write!(f, "{}", self.0)
			}
		}

		impl From<Uuid> for $name {
			fn from(id: Uuid) -> Self {
				Self(id)
			}
		}

		impl From<$name> for Uuid {
			fn from(id: $name) -> Self {
				id.0
			}
		}
	};
}

define_id_type!(UserId, "Unique identifier for a user.");
define_id_type!(SessionId, "Unique identifier for a session.");
define_id_type!(OrgId, "Unique identifier for an organization.");
define_id_type!(TeamId, "Unique identifier for a team.");
define_id_type!(ApiKeyId, "Unique identifier for an API key.");
define_id_type!(InvitationId, "Unique identifier for an invitation.");
define_id_type!(IdentityId, "Unique identifier for a linked identity.");

// =============================================================================
// Global Roles
// =============================================================================

/// System-wide roles that grant elevated permissions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GlobalRole {
	/// Full system access, can promote other admins.
	SystemAdmin,
	/// Can access user data for support purposes (with consent).
	Support,
	/// Read-only access to audit logs.
	Auditor,
}

impl GlobalRole {
	/// Returns all available global roles.
	pub fn all() -> &'static [GlobalRole] {
		&[
			GlobalRole::SystemAdmin,
			GlobalRole::Support,
			GlobalRole::Auditor,
		]
	}
}

impl fmt::Display for GlobalRole {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			GlobalRole::SystemAdmin => write!(f, "system_admin"),
			GlobalRole::Support => write!(f, "support"),
			GlobalRole::Auditor => write!(f, "auditor"),
		}
	}
}

// =============================================================================
// Organization Roles
// =============================================================================

/// Roles within an organization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrgRole {
	/// Full org control, billing, can delete org.
	Owner,
	/// Manage members and settings, cannot delete org.
	Admin,
	/// Standard member access.
	Member,
}

impl OrgRole {
	/// Returns all available organization roles.
	pub fn all() -> &'static [OrgRole] {
		&[OrgRole::Owner, OrgRole::Admin, OrgRole::Member]
	}

	/// Returns true if this role has at least the permissions of the given role.
	pub fn has_permission_of(&self, other: &OrgRole) -> bool {
		matches!(
			(self, other),
			(OrgRole::Owner, _)
				| (OrgRole::Admin, OrgRole::Admin | OrgRole::Member)
				| (OrgRole::Member, OrgRole::Member)
		)
	}
}

impl fmt::Display for OrgRole {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			OrgRole::Owner => write!(f, "owner"),
			OrgRole::Admin => write!(f, "admin"),
			OrgRole::Member => write!(f, "member"),
		}
	}
}

// =============================================================================
// Team Roles
// =============================================================================

/// Roles within a team.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeamRole {
	/// Can manage team members and settings.
	Maintainer,
	/// Standard team member.
	Member,
}

impl TeamRole {
	/// Returns all available team roles.
	pub fn all() -> &'static [TeamRole] {
		&[TeamRole::Maintainer, TeamRole::Member]
	}

	/// Returns true if this role has at least the permissions of the given role.
	pub fn has_permission_of(&self, other: &TeamRole) -> bool {
		matches!(
			(self, other),
			(TeamRole::Maintainer, _) | (TeamRole::Member, TeamRole::Member)
		)
	}
}

impl fmt::Display for TeamRole {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			TeamRole::Maintainer => write!(f, "maintainer"),
			TeamRole::Member => write!(f, "member"),
		}
	}
}

// =============================================================================
// Session Types
// =============================================================================

/// The type of session/client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionType {
	/// Web browser session (cookie-based).
	Web,
	/// CLI session (bearer token).
	Cli,
	/// VS Code extension session (bearer token).
	VsCode,
}

impl fmt::Display for SessionType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			SessionType::Web => write!(f, "web"),
			SessionType::Cli => write!(f, "cli"),
			SessionType::VsCode => write!(f, "vscode"),
		}
	}
}

// =============================================================================
// Visibility
// =============================================================================

/// Visibility level for resources (threads, workspaces, etc.).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
	/// Only the owner can access.
	#[default]
	Private,
	/// Visible to specific team(s).
	Team,
	/// Visible to all organization members.
	Organization,
	/// Visible to everyone (with link).
	Public,
}

impl fmt::Display for Visibility {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Visibility::Private => write!(f, "private"),
			Visibility::Team => write!(f, "team"),
			Visibility::Organization => write!(f, "organization"),
			Visibility::Public => write!(f, "public"),
		}
	}
}

// =============================================================================
// API Key Scopes
// =============================================================================

/// Scopes that can be granted to API keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyScope {
	/// Read thread data.
	ThreadsRead,
	/// Create and update threads.
	ThreadsWrite,
	/// Delete threads.
	ThreadsDelete,
	/// Use LLM services.
	LlmUse,
	/// Execute tools.
	ToolsUse,
}

impl ApiKeyScope {
	/// Returns all available API key scopes.
	pub fn all() -> &'static [ApiKeyScope] {
		&[
			ApiKeyScope::ThreadsRead,
			ApiKeyScope::ThreadsWrite,
			ApiKeyScope::ThreadsDelete,
			ApiKeyScope::LlmUse,
			ApiKeyScope::ToolsUse,
		]
	}
}

impl fmt::Display for ApiKeyScope {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			ApiKeyScope::ThreadsRead => write!(f, "threads:read"),
			ApiKeyScope::ThreadsWrite => write!(f, "threads:write"),
			ApiKeyScope::ThreadsDelete => write!(f, "threads:delete"),
			ApiKeyScope::LlmUse => write!(f, "llm:use"),
			ApiKeyScope::ToolsUse => write!(f, "tools:use"),
		}
	}
}

// =============================================================================
// OAuth Provider
// =============================================================================

/// Supported OAuth providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OAuthProvider {
	/// GitHub OAuth.
	#[serde(rename = "github")]
	GitHub,
	/// Google OAuth.
	#[serde(rename = "google")]
	Google,
}

impl fmt::Display for OAuthProvider {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			OAuthProvider::GitHub => write!(f, "github"),
			OAuthProvider::Google => write!(f, "google"),
		}
	}
}

// =============================================================================
// Identity Provider
// =============================================================================

/// How a user identity was established.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum IdentityProvider {
	/// OAuth-based identity.
	#[serde(rename = "oauth")]
	OAuth {
		provider: OAuthProvider,
		provider_user_id: String,
	},
	/// Email magic link identity.
	#[serde(rename = "magic_link")]
	MagicLink { email: String },
}

impl fmt::Display for IdentityProvider {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			IdentityProvider::OAuth { provider, .. } => write!(f, "oauth:{provider}"),
			IdentityProvider::MagicLink { .. } => write!(f, "magic_link"),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	mod id_types {
		use super::*;

		#[test]
		fn user_id_roundtrips() {
			let uuid = Uuid::new_v4();
			let user_id = UserId::new(uuid);
			assert_eq!(user_id.into_inner(), uuid);
		}

		#[test]
		fn user_id_generates_unique() {
			let id1 = UserId::generate();
			let id2 = UserId::generate();
			assert_ne!(id1, id2);
		}

		#[test]
		fn user_id_serializes_as_uuid() {
			let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
			let user_id = UserId::new(uuid);
			let json = serde_json::to_string(&user_id).unwrap();
			assert_eq!(json, "\"550e8400-e29b-41d4-a716-446655440000\"");
		}

		#[test]
		fn user_id_deserializes_from_uuid() {
			let json = "\"550e8400-e29b-41d4-a716-446655440000\"";
			let user_id: UserId = serde_json::from_str(json).unwrap();
			assert_eq!(
				user_id.into_inner(),
				Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()
			);
		}

		proptest! {
				#[test]
				fn user_id_roundtrip_any_uuid(
						a: u128
				) {
						let uuid = Uuid::from_u128(a);
						let user_id = UserId::new(uuid);
						prop_assert_eq!(user_id.into_inner(), uuid);
						prop_assert_eq!(Uuid::from(user_id), uuid);
				}

				#[test]
				fn session_id_roundtrip_any_uuid(
						a: u128
				) {
						let uuid = Uuid::from_u128(a);
						let session_id = SessionId::new(uuid);
						prop_assert_eq!(session_id.into_inner(), uuid);
				}

				#[test]
				fn org_id_roundtrip_any_uuid(
						a: u128
				) {
						let uuid = Uuid::from_u128(a);
						let org_id = OrgId::new(uuid);
						prop_assert_eq!(org_id.into_inner(), uuid);
				}

				#[test]
				fn user_id_serde_roundtrip(
						a: u128
				) {
						let uuid = Uuid::from_u128(a);
						let user_id = UserId::new(uuid);
						let json = serde_json::to_string(&user_id).unwrap();
						let deserialized: UserId = serde_json::from_str(&json).unwrap();
						prop_assert_eq!(user_id, deserialized);
				}

				#[test]
				fn user_id_display_matches_uuid(
						a: u128
				) {
						let uuid = Uuid::from_u128(a);
						let user_id = UserId::new(uuid);
						prop_assert_eq!(user_id.to_string(), uuid.to_string());
				}
		}
	}

	mod roles {
		use super::*;

		#[test]
		fn org_role_permission_hierarchy() {
			assert!(OrgRole::Owner.has_permission_of(&OrgRole::Owner));
			assert!(OrgRole::Owner.has_permission_of(&OrgRole::Admin));
			assert!(OrgRole::Owner.has_permission_of(&OrgRole::Member));

			assert!(!OrgRole::Admin.has_permission_of(&OrgRole::Owner));
			assert!(OrgRole::Admin.has_permission_of(&OrgRole::Admin));
			assert!(OrgRole::Admin.has_permission_of(&OrgRole::Member));

			assert!(!OrgRole::Member.has_permission_of(&OrgRole::Owner));
			assert!(!OrgRole::Member.has_permission_of(&OrgRole::Admin));
			assert!(OrgRole::Member.has_permission_of(&OrgRole::Member));
		}

		#[test]
		fn team_role_permission_hierarchy() {
			assert!(TeamRole::Maintainer.has_permission_of(&TeamRole::Maintainer));
			assert!(TeamRole::Maintainer.has_permission_of(&TeamRole::Member));

			assert!(!TeamRole::Member.has_permission_of(&TeamRole::Maintainer));
			assert!(TeamRole::Member.has_permission_of(&TeamRole::Member));
		}

		#[test]
		fn global_role_serializes_snake_case() {
			let role = GlobalRole::SystemAdmin;
			let json = serde_json::to_string(&role).unwrap();
			assert_eq!(json, "\"system_admin\"");
		}
	}

	mod visibility {
		use super::*;

		#[test]
		fn default_is_private() {
			assert_eq!(Visibility::default(), Visibility::Private);
		}

		#[test]
		fn serializes_snake_case() {
			let vis = Visibility::Organization;
			let json = serde_json::to_string(&vis).unwrap();
			assert_eq!(json, "\"organization\"");
		}
	}

	mod api_key_scope {
		use super::*;

		#[test]
		fn all_returns_all_scopes() {
			assert_eq!(ApiKeyScope::all().len(), 5);
		}

		#[test]
		fn display_uses_colon_separator() {
			assert_eq!(ApiKeyScope::ThreadsRead.to_string(), "threads:read");
			assert_eq!(ApiKeyScope::LlmUse.to_string(), "llm:use");
		}
	}

	mod identity_provider {
		use super::*;

		#[test]
		fn oauth_serializes_with_tag() {
			let provider = IdentityProvider::OAuth {
				provider: OAuthProvider::GitHub,
				provider_user_id: "12345".to_string(),
			};
			let json = serde_json::to_string(&provider).unwrap();
			assert!(json.contains("\"type\":\"oauth\""), "got: {json}");
			assert!(json.contains("\"provider\":\"github\""), "got: {json}");
		}

		#[test]
		fn magic_link_serializes_with_tag() {
			let provider = IdentityProvider::MagicLink {
				email: "user@example.com".to_string(),
			};
			let json = serde_json::to_string(&provider).unwrap();
			assert!(json.contains("\"type\":\"magic_link\""));
		}
	}
}
