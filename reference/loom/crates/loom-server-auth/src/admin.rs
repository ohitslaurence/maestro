// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Admin operations and impersonation for system administrators.
//!
//! This module provides:
//! - Admin promotion/demotion checks
//! - Impersonation session management with full audit trail
//!
//! # Bootstrap
//!
//! The first user to register becomes the system admin (bootstrap).
//!
//! # Impersonation
//!
//! System admins can impersonate other users for debugging/support purposes.
//! All impersonation sessions are fully audited.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AuthError;
use crate::types::UserId;
use crate::user::User;

/// An impersonation session where an admin assumes another user's identity.
///
/// All impersonation sessions are logged for audit purposes. The admin
/// retains their original identity but operates as the target user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpersonationSession {
	/// Unique identifier for this impersonation session.
	pub id: Uuid,

	/// The real admin user performing the impersonation.
	pub admin_user_id: UserId,

	/// The user being impersonated.
	pub target_user_id: UserId,

	/// When the impersonation session started.
	pub started_at: DateTime<Utc>,

	/// When the impersonation session ended (None if still active).
	pub ended_at: Option<DateTime<Utc>>,

	/// Optional reason for the impersonation (e.g., support ticket ID).
	pub reason: Option<String>,
}

impl ImpersonationSession {
	/// Create a new impersonation session.
	///
	/// # Arguments
	///
	/// * `admin_user_id` - The ID of the admin performing the impersonation
	/// * `target_user_id` - The ID of the user being impersonated
	/// * `reason` - Optional reason for the impersonation
	pub fn new(admin_user_id: UserId, target_user_id: UserId, reason: Option<String>) -> Self {
		Self {
			id: Uuid::new_v4(),
			admin_user_id,
			target_user_id,
			started_at: Utc::now(),
			ended_at: None,
			reason,
		}
	}

	/// End this impersonation session.
	///
	/// Sets the `ended_at` timestamp to the current time.
	pub fn end(&mut self) {
		self.ended_at = Some(Utc::now());
	}

	/// Check if this impersonation session is still active.
	///
	/// A session is active if `ended_at` is `None`.
	pub fn is_active(&self) -> bool {
		self.ended_at.is_none()
	}

	/// Get the duration of this session.
	///
	/// Returns the duration from start to end, or from start to now if still active.
	pub fn duration(&self) -> chrono::Duration {
		let end = self.ended_at.unwrap_or_else(Utc::now);
		end - self.started_at
	}
}

/// Check if the actor can promote another user to system admin.
///
/// Only system admins can promote other users.
///
/// # Errors
///
/// Returns `AuthError::Forbidden` if the actor is not a system admin.
pub fn check_can_promote(actor: &User) -> Result<(), AuthError> {
	if !actor.is_system_admin() {
		return Err(AuthError::Forbidden(
			"Only system admins can promote users".into(),
		));
	}
	Ok(())
}

/// Check if the actor can demote the target from system admin.
///
/// # Rules
///
/// - Only system admins can demote other admins
/// - Cannot demote yourself
/// - Must keep at least one system admin
///
/// # Arguments
///
/// * `actor` - The user attempting the demotion
/// * `target` - The user being demoted
/// * `remaining_admins` - The number of system admins that would remain after demotion
///
/// # Errors
///
/// Returns `AuthError::Forbidden` if:
/// - Actor is not a system admin
/// - Actor is trying to demote themselves
/// - Demotion would leave no system admins
pub fn check_can_demote(
	actor: &User,
	target: &User,
	remaining_admins: usize,
) -> Result<(), AuthError> {
	if !actor.is_system_admin() {
		return Err(AuthError::Forbidden(
			"Only system admins can demote users".into(),
		));
	}
	if actor.id == target.id {
		return Err(AuthError::Forbidden("Cannot demote yourself".into()));
	}
	if remaining_admins <= 1 {
		return Err(AuthError::Forbidden(
			"Must have at least one system admin".into(),
		));
	}
	Ok(())
}

/// Check if the actor can impersonate another user.
///
/// Only system admins can impersonate other users.
///
/// # Errors
///
/// Returns `AuthError::Forbidden` if the actor is not a system admin.
pub fn check_can_impersonate(actor: &User) -> Result<(), AuthError> {
	if !actor.is_system_admin() {
		return Err(AuthError::Forbidden(
			"Only system admins can impersonate users".into(),
		));
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::Utc;

	fn make_test_user(is_admin: bool) -> User {
		User {
			id: UserId::generate(),
			display_name: "Test User".to_string(),
			username: None,
			primary_email: Some("test@example.com".to_string()),
			avatar_url: None,
			email_visible: true,
			is_system_admin: is_admin,
			is_support: false,
			is_auditor: false,
			created_at: Utc::now(),
			updated_at: Utc::now(),
			deleted_at: None,
			locale: None,
		}
	}

	mod impersonation_session {
		use super::*;

		#[test]
		fn new_creates_active_session() {
			let admin_id = UserId::generate();
			let target_id = UserId::generate();

			let session =
				ImpersonationSession::new(admin_id, target_id, Some("Support ticket #123".to_string()));

			assert_eq!(session.admin_user_id, admin_id);
			assert_eq!(session.target_user_id, target_id);
			assert!(session.is_active());
			assert!(session.ended_at.is_none());
			assert_eq!(session.reason.as_deref(), Some("Support ticket #123"));
		}

		#[test]
		fn new_without_reason() {
			let admin_id = UserId::generate();
			let target_id = UserId::generate();

			let session = ImpersonationSession::new(admin_id, target_id, None);

			assert!(session.reason.is_none());
		}

		#[test]
		fn end_sets_ended_at() {
			let mut session = ImpersonationSession::new(UserId::generate(), UserId::generate(), None);

			assert!(session.is_active());
			session.end();
			assert!(!session.is_active());
			assert!(session.ended_at.is_some());
		}

		#[test]
		fn is_active_returns_false_after_end() {
			let mut session = ImpersonationSession::new(UserId::generate(), UserId::generate(), None);

			session.end();
			assert!(!session.is_active());
		}

		#[test]
		fn duration_for_active_session() {
			let session = ImpersonationSession::new(UserId::generate(), UserId::generate(), None);

			let duration = session.duration();
			assert!(duration.num_milliseconds() >= 0);
		}

		#[test]
		fn duration_for_ended_session() {
			let mut session = ImpersonationSession::new(UserId::generate(), UserId::generate(), None);
			session.end();

			let duration = session.duration();
			assert!(duration.num_milliseconds() >= 0);
		}

		#[test]
		fn serializes_to_json() {
			let session = ImpersonationSession::new(
				UserId::generate(),
				UserId::generate(),
				Some("Test reason".to_string()),
			);

			let json = serde_json::to_string(&session).unwrap();
			assert!(json.contains("admin_user_id"));
			assert!(json.contains("target_user_id"));
			assert!(json.contains("started_at"));
			assert!(json.contains("Test reason"));
		}

		#[test]
		fn deserializes_from_json() {
			let session = ImpersonationSession::new(UserId::generate(), UserId::generate(), None);

			let json = serde_json::to_string(&session).unwrap();
			let deserialized: ImpersonationSession = serde_json::from_str(&json).unwrap();

			assert_eq!(deserialized.id, session.id);
			assert_eq!(deserialized.admin_user_id, session.admin_user_id);
			assert_eq!(deserialized.target_user_id, session.target_user_id);
		}
	}

	mod check_can_promote {
		use super::*;

		#[test]
		fn allows_system_admin() {
			let admin = make_test_user(true);
			assert!(check_can_promote(&admin).is_ok());
		}

		#[test]
		fn denies_non_admin() {
			let user = make_test_user(false);
			let result = check_can_promote(&user);

			assert!(result.is_err());
			let err = result.unwrap_err();
			assert!(matches!(err, AuthError::Forbidden(_)));
			assert!(err.to_string().contains("Only system admins can promote"));
		}
	}

	mod check_can_demote {
		use super::*;

		#[test]
		fn allows_admin_demoting_other_admin() {
			let actor = make_test_user(true);
			let target = make_test_user(true);

			let result = check_can_demote(&actor, &target, 3);
			assert!(result.is_ok());
		}

		#[test]
		fn denies_non_admin() {
			let actor = make_test_user(false);
			let target = make_test_user(true);

			let result = check_can_demote(&actor, &target, 3);
			assert!(result.is_err());
			assert!(matches!(result.unwrap_err(), AuthError::Forbidden(_)));
		}

		#[test]
		fn denies_self_demotion() {
			let mut actor = make_test_user(true);
			let target_id = actor.id;
			actor.id = target_id;

			let result = check_can_demote(&actor, &actor, 3);
			assert!(result.is_err());
			let err = result.unwrap_err();
			assert!(matches!(err, AuthError::Forbidden(_)));
			assert!(err.to_string().contains("Cannot demote yourself"));
		}

		#[test]
		fn denies_when_last_admin() {
			let actor = make_test_user(true);
			let target = make_test_user(true);

			let result = check_can_demote(&actor, &target, 1);
			assert!(result.is_err());
			let err = result.unwrap_err();
			assert!(matches!(err, AuthError::Forbidden(_)));
			assert!(err.to_string().contains("at least one system admin"));
		}

		#[test]
		fn denies_when_zero_admins_remaining() {
			let actor = make_test_user(true);
			let target = make_test_user(true);

			let result = check_can_demote(&actor, &target, 0);
			assert!(result.is_err());
		}

		#[test]
		fn allows_when_two_admins_remaining() {
			let actor = make_test_user(true);
			let target = make_test_user(true);

			let result = check_can_demote(&actor, &target, 2);
			assert!(result.is_ok());
		}
	}

	mod check_can_impersonate {
		use super::*;

		#[test]
		fn allows_system_admin() {
			let admin = make_test_user(true);
			assert!(check_can_impersonate(&admin).is_ok());
		}

		#[test]
		fn denies_non_admin() {
			let user = make_test_user(false);
			let result = check_can_impersonate(&user);

			assert!(result.is_err());
			let err = result.unwrap_err();
			assert!(matches!(err, AuthError::Forbidden(_)));
			assert!(err
				.to_string()
				.contains("Only system admins can impersonate"));
		}

		#[test]
		fn denies_support_user() {
			let mut user = make_test_user(false);
			user.is_support = true;

			let result = check_can_impersonate(&user);
			assert!(result.is_err());
		}

		#[test]
		fn denies_auditor() {
			let mut user = make_test_user(false);
			user.is_auditor = true;

			let result = check_can_impersonate(&user);
			assert!(result.is_err());
		}
	}
}
