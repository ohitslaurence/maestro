// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Support access management for thread debugging.
//!
//! Support access allows support staff to request temporary access to a user's
//! thread for debugging purposes. Access requires user approval and auto-expires
//! after 31 days.

use crate::UserId;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Number of days support access remains valid after approval.
pub const SUPPORT_ACCESS_DAYS: i64 = 31;

/// A support access request/grant for a thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportAccess {
	/// Unique identifier for this access request.
	pub id: Uuid,
	/// Thread this access request is for.
	pub thread_id: String,
	/// Support user who requested access.
	pub requested_by: UserId,
	/// Thread owner who approved access (None if pending).
	pub approved_by: Option<UserId>,
	/// When the request was made.
	pub requested_at: DateTime<Utc>,
	/// When the request was approved (None if pending).
	pub approved_at: Option<DateTime<Utc>>,
	/// When the access expires (31 days after approval, None if pending).
	pub expires_at: Option<DateTime<Utc>>,
	/// When the access was revoked (None if active or pending).
	pub revoked_at: Option<DateTime<Utc>>,
}

impl SupportAccess {
	/// Create a new support access request.
	pub fn new(thread_id: impl Into<String>, requested_by: UserId) -> Self {
		Self {
			id: Uuid::new_v4(),
			thread_id: thread_id.into(),
			requested_by,
			approved_by: None,
			requested_at: Utc::now(),
			approved_at: None,
			expires_at: None,
			revoked_at: None,
		}
	}

	/// Check if the request is pending approval.
	pub fn is_pending(&self) -> bool {
		self.approved_at.is_none() && self.revoked_at.is_none()
	}

	/// Check if the request has been approved.
	pub fn is_approved(&self) -> bool {
		self.approved_at.is_some()
	}

	/// Check if the access is currently active (approved, not expired, not revoked).
	pub fn is_active(&self) -> bool {
		if self.approved_at.is_none() {
			return false;
		}
		if self.revoked_at.is_some() {
			return false;
		}
		if let Some(expires_at) = self.expires_at {
			if Utc::now() >= expires_at {
				return false;
			}
		}
		true
	}

	/// Approve the access request.
	///
	/// Sets the approved_by, approved_at, and expires_at (31 days from now).
	pub fn approve(&mut self, by: UserId) {
		let now = Utc::now();
		self.approved_by = Some(by);
		self.approved_at = Some(now);
		self.expires_at = Some(now + Duration::days(SUPPORT_ACCESS_DAYS));
	}

	/// Revoke the access (either pending or approved).
	pub fn revoke(&mut self) {
		self.revoked_at = Some(Utc::now());
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	mod support_access_struct {
		use super::*;

		#[test]
		fn new_creates_pending_request() {
			let support_user = UserId::generate();
			let access = SupportAccess::new("T-test-thread", support_user);

			assert!(access.is_pending());
			assert!(!access.is_approved());
			assert!(!access.is_active());
			assert_eq!(access.thread_id, "T-test-thread");
			assert_eq!(access.requested_by, support_user);
			assert!(access.approved_by.is_none());
			assert!(access.approved_at.is_none());
			assert!(access.expires_at.is_none());
			assert!(access.revoked_at.is_none());
		}

		#[test]
		fn approve_sets_expiry() {
			let support_user = UserId::generate();
			let owner = UserId::generate();
			let mut access = SupportAccess::new("T-test-thread", support_user);

			let before_approve = Utc::now();
			access.approve(owner);
			let after_approve = Utc::now();

			assert!(!access.is_pending());
			assert!(access.is_approved());
			assert!(access.is_active());
			assert_eq!(access.approved_by, Some(owner));
			assert!(access.approved_at.is_some());
			assert!(access.expires_at.is_some());

			let expires = access.expires_at.unwrap();
			let expected_min = before_approve + Duration::days(SUPPORT_ACCESS_DAYS);
			let expected_max = after_approve + Duration::days(SUPPORT_ACCESS_DAYS);
			assert!(expires >= expected_min && expires <= expected_max);
		}

		#[test]
		fn revoke_pending_request() {
			let support_user = UserId::generate();
			let mut access = SupportAccess::new("T-test-thread", support_user);

			access.revoke();

			assert!(!access.is_pending());
			assert!(!access.is_approved());
			assert!(!access.is_active());
			assert!(access.revoked_at.is_some());
		}

		#[test]
		fn revoke_approved_access() {
			let support_user = UserId::generate();
			let owner = UserId::generate();
			let mut access = SupportAccess::new("T-test-thread", support_user);

			access.approve(owner);
			assert!(access.is_active());

			access.revoke();
			assert!(!access.is_active());
			assert!(access.revoked_at.is_some());
		}

		#[test]
		fn expired_access_is_not_active() {
			let support_user = UserId::generate();
			let owner = UserId::generate();
			let mut access = SupportAccess::new("T-test-thread", support_user);

			access.approve(owner);
			// Manually set expires_at to the past
			access.expires_at = Some(Utc::now() - Duration::hours(1));

			assert!(access.is_approved());
			assert!(!access.is_active());
		}

		#[test]
		fn support_access_days_is_31() {
			assert_eq!(SUPPORT_ACCESS_DAYS, 31);
		}
	}
}
