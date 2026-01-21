// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! User management types and operations.
//!
//! This module provides:
//! - [`User`] - core user entity with global roles and soft-delete
//! - [`UserProfile`] - public view of a user (respects email visibility)
//! - [`Identity`] - OAuth provider linkages
//! - [`Provider`] - authentication provider enum

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::types::{GlobalRole, IdentityId, UserId};

/// Reserved usernames that cannot be used.
/// These are reserved for system use, common services, or could cause confusion.
pub const RESERVED_USERNAMES: &[&str] = &[
	// System/admin
	"root",
	"admin",
	"administrator",
	"sudo",
	"system",
	"sysadmin",
	"postmaster",
	"webmaster",
	"hostmaster",
	"abuse",
	"security",
	"support",
	"help",
	"info",
	"contact",
	"noreply",
	"no-reply",
	// Git/version control
	"git",
	"jj",
	"svn",
	"hg",
	"mercurial",
	"cvs",
	"github",
	"gitlab",
	"bitbucket",
	"gitea",
	"forgejo",
	"sourcehut",
	// Loom-specific
	"loom",
	"weaver",
	"weavers",
	"thread",
	"threads",
	"cannon",
	"mirrors",
	"mirror",
	"api",
	"oauth",
	"auth",
	"login",
	"logout",
	"signup",
	"register",
	"settings",
	"profile",
	"account",
	// Web/static assets
	"static",
	"assets",
	"images",
	"img",
	"css",
	"js",
	"fonts",
	"public",
	"private",
	"uploads",
	"downloads",
	"files",
	"favicon",
	"robots",
	"sitemap",
	"manifest",
	// Social/services
	"discord",
	"slack",
	"twitter",
	"x",
	"instagram",
	"facebook",
	"linkedin",
	"youtube",
	"tiktok",
	"reddit",
	"chat",
	"status",
	// Common paths
	"www",
	"web",
	"app",
	"apps",
	"home",
	"index",
	"default",
	"new",
	"edit",
	"delete",
	"create",
	"update",
	"search",
	"explore",
	"discover",
	"trending",
	"popular",
	"featured",
	// Organizations/teams
	"org",
	"orgs",
	"organization",
	"organizations",
	"team",
	"teams",
	"group",
	"groups",
	"enterprise",
	// Misc reserved
	"null",
	"undefined",
	"none",
	"anonymous",
	"guest",
	"user",
	"users",
	"test",
	"testing",
	"demo",
	"example",
	"sample",
	"blog",
	"news",
	"docs",
	"documentation",
	"wiki",
	"forum",
	"pricing",
	"about",
	"terms",
	"privacy",
	"legal",
	"dmca",
];

/// Check if a username is reserved.
pub fn is_username_reserved(username: &str) -> bool {
	let lower = username.to_lowercase();
	RESERVED_USERNAMES.iter().any(|&reserved| reserved == lower)
}

/// A user in the system.
///
/// Users can authenticate via multiple identity providers and may have
/// global roles that grant elevated permissions across the platform.
///
/// # PII Handling
///
/// This struct contains personally identifiable information (PII) that
/// requires careful handling:
/// - `display_name` and `primary_email` are user-provided PII
/// - These fields should be redacted in logs
/// - Data retention policies apply (see account_deletion module)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
	/// Unique identifier for this user.
	pub id: UserId,

	/// Display name shown in the UI.
	pub display_name: String,

	/// Unique username for git URLs, profile URLs, etc.
	pub username: Option<String>,

	/// Primary email address for notifications.
	/// Users may have multiple emails via different identities,
	/// but only this one receives notifications.
	pub primary_email: Option<String>,

	/// URL to the user's avatar image.
	pub avatar_url: Option<String>,

	/// Whether the user's email is visible to other users.
	pub email_visible: bool,

	/// Full system access, can promote other admins.
	pub is_system_admin: bool,

	/// Can access user data for support purposes (with consent).
	pub is_support: bool,

	/// Read-only access to audit logs.
	pub is_auditor: bool,

	/// When the user was created.
	pub created_at: DateTime<Utc>,

	/// When the user was last updated.
	pub updated_at: DateTime<Utc>,

	/// When the user was soft-deleted, if applicable.
	/// Users with a deleted_at timestamp are considered inactive.
	pub deleted_at: Option<DateTime<Utc>>,

	/// User's preferred locale for emails and notifications.
	/// ISO 639-1 language code (e.g., "en", "es", "ar").
	/// None means use server default.
	pub locale: Option<String>,
}

impl User {
	/// Returns true if this user has been soft-deleted.
	pub fn is_deleted(&self) -> bool {
		self.deleted_at.is_some()
	}

	/// Returns true if this user has the specified global role.
	pub fn has_global_role(&self, role: GlobalRole) -> bool {
		match role {
			GlobalRole::SystemAdmin => self.is_system_admin,
			GlobalRole::Support => self.is_support,
			GlobalRole::Auditor => self.is_auditor,
		}
	}

	/// Returns true if this user is a system administrator.
	pub fn is_system_admin(&self) -> bool {
		self.is_system_admin
	}

	/// Returns true if this user has support access.
	pub fn is_support(&self) -> bool {
		self.is_support
	}

	/// Returns true if this user is an auditor.
	pub fn is_auditor(&self) -> bool {
		self.is_auditor
	}

	/// Returns all global roles this user has.
	pub fn global_roles(&self) -> Vec<GlobalRole> {
		let mut roles = Vec::new();
		if self.is_system_admin {
			roles.push(GlobalRole::SystemAdmin);
		}
		if self.is_support {
			roles.push(GlobalRole::Support);
		}
		if self.is_auditor {
			roles.push(GlobalRole::Auditor);
		}
		roles
	}

	/// Creates a public profile view of this user.
	/// Respects the email_visible setting.
	pub fn to_profile(&self) -> UserProfile {
		UserProfile {
			id: self.id,
			display_name: self.display_name.clone(),
			email: if self.email_visible {
				self.primary_email.clone()
			} else {
				None
			},
			avatar_url: self.avatar_url.clone(),
		}
	}
}

/// Public view of a user profile.
///
/// This struct respects the user's email visibility settings
/// and only includes information safe to share with other users.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UserProfile {
	/// Unique identifier for this user.
	pub id: UserId,

	/// Display name shown in the UI.
	pub display_name: String,

	/// Email address (only if user has email_visible enabled).
	pub email: Option<String>,

	/// URL to the user's avatar image.
	pub avatar_url: Option<String>,
}

/// A linked identity from an authentication provider.
///
/// Users can have multiple identities (e.g., GitHub + Google + MagicLink)
/// all linked to the same user account.
///
/// # PII Handling
///
/// This struct contains PII from external providers:
/// - `email` is user PII from the OAuth provider
/// - `provider_user_id` may be linkable to external accounts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
	/// Unique identifier for this identity.
	pub id: IdentityId,

	/// The user this identity belongs to.
	pub user_id: UserId,

	/// The authentication provider.
	pub provider: Provider,

	/// The user's ID at the provider (e.g., GitHub user ID).
	pub provider_user_id: String,

	/// Email address from this provider.
	pub email: String,

	/// Whether the provider has verified this email.
	pub email_verified: bool,

	/// When this identity was created.
	pub created_at: DateTime<Utc>,
}

/// Authentication provider types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
	/// GitHub OAuth.
	GitHub,
	/// Google OAuth.
	Google,
	/// Email magic link (passwordless).
	MagicLink,
}

impl std::fmt::Display for Provider {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Provider::GitHub => write!(f, "github"),
			Provider::Google => write!(f, "google"),
			Provider::MagicLink => write!(f, "magic_link"),
		}
	}
}

/// Validates a username.
/// Rules:
/// - 3-39 characters
/// - Alphanumeric and underscores only
/// - Cannot start with underscore
/// - Cannot be all numeric
/// - Cannot be a reserved username
pub fn validate_username(username: &str) -> Result<(), &'static str> {
	if username.len() < 3 {
		return Err("Username must be at least 3 characters");
	}
	if username.len() > 39 {
		return Err("Username must be at most 39 characters");
	}
	if !username
		.chars()
		.all(|c| c.is_ascii_alphanumeric() || c == '_')
	{
		return Err("Username can only contain letters, numbers, and underscores");
	}
	if username.starts_with('_') {
		return Err("Username cannot start with underscore");
	}
	if username.chars().all(|c| c.is_ascii_digit()) {
		return Err("Username cannot be all numbers");
	}
	if is_username_reserved(username) {
		return Err("This username is reserved");
	}
	Ok(())
}

/// Generates a username from a display name or email.
/// Sanitizes to alphanumeric + underscore, lowercases.
pub fn generate_username_base(source: &str) -> String {
	let base = if let Some(at_pos) = source.find('@') {
		&source[..at_pos]
	} else {
		source
	};

	let sanitized: String = base
		.chars()
		.map(|c| {
			if c.is_ascii_alphanumeric() {
				c.to_ascii_lowercase()
			} else {
				'_'
			}
		})
		.collect();

	let collapsed: String = sanitized
		.trim_start_matches('_')
		.split('_')
		.filter(|s| !s.is_empty())
		.collect::<Vec<_>>()
		.join("_");

	if collapsed.len() < 3 {
		format!("user_{}", collapsed)
	} else if collapsed.len() > 39 {
		collapsed[..39].to_string()
	} else {
		collapsed
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_test_user() -> User {
		User {
			id: UserId::generate(),
			display_name: "Test User".to_string(),
			username: None,
			primary_email: Some("test@example.com".to_string()),
			avatar_url: Some("https://example.com/avatar.png".to_string()),
			email_visible: true,
			is_system_admin: false,
			is_support: false,
			is_auditor: false,
			created_at: Utc::now(),
			updated_at: Utc::now(),
			deleted_at: None,
			locale: None,
		}
	}

	mod user {
		use super::*;

		#[test]
		fn is_deleted_returns_false_when_no_deleted_at() {
			let user = make_test_user();
			assert!(!user.is_deleted());
		}

		#[test]
		fn is_deleted_returns_true_when_deleted_at_set() {
			let mut user = make_test_user();
			user.deleted_at = Some(Utc::now());
			assert!(user.is_deleted());
		}

		#[test]
		fn has_global_role_system_admin() {
			let mut user = make_test_user();
			assert!(!user.has_global_role(GlobalRole::SystemAdmin));

			user.is_system_admin = true;
			assert!(user.has_global_role(GlobalRole::SystemAdmin));
		}

		#[test]
		fn has_global_role_support() {
			let mut user = make_test_user();
			assert!(!user.has_global_role(GlobalRole::Support));

			user.is_support = true;
			assert!(user.has_global_role(GlobalRole::Support));
		}

		#[test]
		fn has_global_role_auditor() {
			let mut user = make_test_user();
			assert!(!user.has_global_role(GlobalRole::Auditor));

			user.is_auditor = true;
			assert!(user.has_global_role(GlobalRole::Auditor));
		}

		#[test]
		fn is_system_admin_method() {
			let mut user = make_test_user();
			assert!(!user.is_system_admin());

			user.is_system_admin = true;
			assert!(user.is_system_admin());
		}

		#[test]
		fn is_support_method() {
			let mut user = make_test_user();
			assert!(!user.is_support());

			user.is_support = true;
			assert!(user.is_support());
		}

		#[test]
		fn is_auditor_method() {
			let mut user = make_test_user();
			assert!(!user.is_auditor());

			user.is_auditor = true;
			assert!(user.is_auditor());
		}

		#[test]
		fn global_roles_returns_empty_for_regular_user() {
			let user = make_test_user();
			assert!(user.global_roles().is_empty());
		}

		#[test]
		fn global_roles_returns_all_assigned_roles() {
			let mut user = make_test_user();
			user.is_system_admin = true;
			user.is_auditor = true;

			let roles = user.global_roles();
			assert_eq!(roles.len(), 2);
			assert!(roles.contains(&GlobalRole::SystemAdmin));
			assert!(roles.contains(&GlobalRole::Auditor));
		}

		#[test]
		fn to_profile_includes_email_when_visible() {
			let user = make_test_user();
			let profile = user.to_profile();

			assert_eq!(profile.id, user.id);
			assert_eq!(profile.display_name, user.display_name);
			assert_eq!(profile.email, user.primary_email);
			assert_eq!(profile.avatar_url, user.avatar_url);
		}

		#[test]
		fn to_profile_hides_email_when_not_visible() {
			let mut user = make_test_user();
			user.email_visible = false;

			let profile = user.to_profile();
			assert!(profile.email.is_none());
		}
	}

	mod identity {
		use super::*;

		#[test]
		fn identity_creation() {
			let identity = Identity {
				id: IdentityId::generate(),
				user_id: UserId::generate(),
				provider: Provider::GitHub,
				provider_user_id: "12345".to_string(),
				email: "test@example.com".to_string(),
				email_verified: true,
				created_at: Utc::now(),
			};

			assert_eq!(identity.provider, Provider::GitHub);
			assert!(identity.email_verified);
		}
	}

	mod provider {
		use super::*;

		#[test]
		fn display_formatting() {
			assert_eq!(Provider::GitHub.to_string(), "github");
			assert_eq!(Provider::Google.to_string(), "google");
			assert_eq!(Provider::MagicLink.to_string(), "magic_link");
		}

		#[test]
		fn serializes_snake_case() {
			let github_json = serde_json::to_string(&Provider::GitHub).unwrap();
			assert_eq!(github_json, "\"git_hub\"");

			let google_json = serde_json::to_string(&Provider::Google).unwrap();
			assert_eq!(google_json, "\"google\"");

			let magic_link_json = serde_json::to_string(&Provider::MagicLink).unwrap();
			assert_eq!(magic_link_json, "\"magic_link\"");
		}

		#[test]
		fn deserializes_snake_case() {
			let github: Provider = serde_json::from_str("\"git_hub\"").unwrap();
			assert_eq!(github, Provider::GitHub);

			let google: Provider = serde_json::from_str("\"google\"").unwrap();
			assert_eq!(google, Provider::Google);

			let magic_link: Provider = serde_json::from_str("\"magic_link\"").unwrap();
			assert_eq!(magic_link, Provider::MagicLink);
		}
	}

	mod user_profile {
		use super::*;
		use uuid::Uuid;

		#[test]
		fn serializes_correctly() {
			let profile = UserProfile {
				id: UserId::new(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap()),
				display_name: "Test User".to_string(),
				email: Some("test@example.com".to_string()),
				avatar_url: None,
			};

			let json = serde_json::to_string(&profile).unwrap();
			assert!(json.contains("\"display_name\":\"Test User\""));
			assert!(json.contains("\"email\":\"test@example.com\""));
		}
	}

	mod validate_username {
		use super::*;

		#[test]
		fn test_validate_username_valid() {
			assert!(validate_username("alice").is_ok());
			assert!(validate_username("bob123").is_ok());
			assert!(validate_username("user_name").is_ok());
			assert!(validate_username("a1b").is_ok());
		}

		#[test]
		fn test_validate_username_too_short() {
			assert!(validate_username("ab").is_err());
			assert!(validate_username("a").is_err());
			assert!(validate_username("").is_err());
		}

		#[test]
		fn test_validate_username_too_long() {
			let long = "a".repeat(40);
			assert!(validate_username(&long).is_err());
		}

		#[test]
		fn test_validate_username_invalid_chars() {
			assert!(validate_username("user@name").is_err());
			assert!(validate_username("user.name").is_err());
			assert!(validate_username("user-name").is_err());
			assert!(validate_username("user name").is_err());
		}

		#[test]
		fn test_validate_username_starts_with_underscore() {
			assert!(validate_username("_username").is_err());
		}

		#[test]
		fn test_validate_username_all_numbers() {
			assert!(validate_username("12345").is_err());
			assert!(validate_username("123").is_err());
		}

		#[test]
		fn test_validate_username_reserved() {
			assert!(validate_username("root").is_err());
			assert!(validate_username("admin").is_err());
			assert!(validate_username("github").is_err());
		}
	}

	mod reserved_usernames {
		use super::*;

		#[test]
		fn test_reserved_usernames() {
			assert!(is_username_reserved("root"));
			assert!(is_username_reserved("ROOT")); // case insensitive
			assert!(is_username_reserved("admin"));
			assert!(is_username_reserved("github"));
			assert!(is_username_reserved("loom"));
			assert!(!is_username_reserved("alice"));
			assert!(!is_username_reserved("bob123"));
		}
	}

	mod generate_username {
		use super::*;

		#[test]
		fn test_generate_username_from_email() {
			assert_eq!(generate_username_base("alice@example.com"), "alice");
			assert_eq!(generate_username_base("bob.smith@gmail.com"), "bob_smith");
		}

		#[test]
		fn test_generate_username_from_name() {
			assert_eq!(generate_username_base("Alice Smith"), "alice_smith");
			assert_eq!(generate_username_base("Bob"), "bob");
		}

		#[test]
		fn test_generate_username_special_chars() {
			assert_eq!(generate_username_base("user@#$%name"), "user");
		}

		#[test]
		fn test_generate_username_long_input() {
			let long = "a".repeat(50);
			let result = generate_username_base(&long);
			assert!(result.len() <= 39);
		}
	}

	mod username_proptests {
		use super::*;
		use proptest::prelude::*;

		proptest! {
				#[test]
				fn prop_valid_username_format(
						prefix in "[a-z][a-z0-9_]{2,20}"
				) {
						prop_assert!(validate_username(&prefix).is_ok());
				}

				#[test]
				fn prop_generated_username_is_valid(
						input in "[a-zA-Z0-9@._\\- ]{1,50}"
				) {
						let generated = generate_username_base(&input);
						prop_assert!(generated.len() >= 3);
						prop_assert!(generated.len() <= 39);
						prop_assert!(generated.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
				}

				#[test]
				fn prop_username_length_bounds(
						len in 0usize..100
				) {
						let username: String = "a".repeat(len);
						if len >= 3 && len <= 39 {
								let _ = validate_username(&username);
						} else {
								prop_assert!(validate_username(&username).is_err());
						}
				}

				#[test]
				fn prop_reserved_usernames_rejected(
						reserved in proptest::sample::select(RESERVED_USERNAMES.to_vec())
				) {
						prop_assert!(validate_username(reserved).is_err());
				}
		}
	}
}
